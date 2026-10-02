use std::{fs::File, path::Path};

use anyhow::{Context as AnyhowContext, Result, anyhow, bail};
use polars::prelude::*;
use polars::sql::SQLContext;

#[derive(Clone, Debug, Default)]
pub struct ColumnStats {
    pub name: String,
    pub dtype: String,
    pub count: usize,
    pub nulls: usize,
    pub unique: usize,
    pub min: String,
    pub max: String,
    pub mean: Option<f64>,
    pub std_dev: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct HistogramBin {
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub stats: Vec<ColumnStats>,
    pub correlation: Option<(String, String, f64)>,
    pub histogram: Vec<HistogramBin>,
}

pub fn load_csv(path: &Path) -> Result<DataFrame> {
    let reader = CsvReadOptions::default()
        .with_has_header(true)
        .with_infer_schema_length(Some(10_000))
        .try_into_reader_with_file_path(Some(path.to_path_buf()))
        .map_err(|error| anyhow!("cannot open {}: {error}", path.display()))?;
    reader
        .finish()
        .map_err(|error| anyhow!("cannot parse CSV {}: {error}", path.display()))
}

pub fn execute_sql(df: &DataFrame, sql: &str) -> Result<DataFrame> {
    let mut ctx = SQLContext::new();
    ctx.register("df", df.clone().lazy());
    Ok(ctx.execute(sql)?.collect()?)
}

pub fn execute_sql_like(df: &DataFrame, input: &str) -> Result<DataFrame> {
    let trimmed = input.trim().trim_end_matches(';').trim();
    let lower = trimmed.to_ascii_lowercase();
    let condition = lower
        .strip_prefix("select where")
        .map(|_| trimmed["select where".len()..].trim())
        .ok_or_else(|| anyhow!("SQL-Like syntax: select where <condition>"))?;
    if condition.is_empty() {
        bail!("SQL-Like condition cannot be empty");
    }
    execute_sql(df, &format!("SELECT * FROM df WHERE {condition}"))
}

pub fn fuzzy(df: &DataFrame, query: &str) -> Result<DataFrame> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(df.clone());
    }
    let mut selected = Vec::new();
    for row_idx in 0..df.height() {
        let row = df.get_row(row_idx)?;
        if row
            .0
            .iter()
            .any(|value| fuzzy_match(&display_value(value), &query))
        {
            selected.push(row_idx as IdxSize);
        }
    }
    Ok(df.take(&IdxCa::from_vec("idx".into(), selected))?)
}

fn fuzzy_match(value: &str, query: &str) -> bool {
    let value = value.to_lowercase();
    if value.contains(query) {
        return true;
    }
    let mut chars = query.chars();
    let mut wanted = chars.next();
    for c in value.chars() {
        if Some(c) == wanted {
            wanted = chars.next();
            if wanted.is_none() {
                return true;
            }
        }
    }
    false
}

pub fn sort_sql(df: &DataFrame, columns: &[String], descending: bool) -> Result<DataFrame> {
    if columns.is_empty() {
        bail!("enter one or more comma-separated columns");
    }
    for name in columns {
        if !df
            .get_column_names()
            .iter()
            .any(|candidate| candidate.as_str() == name)
        {
            bail!("unknown column: {name}");
        }
    }
    let order = if descending { "DESC" } else { "ASC" };
    let fields = columns
        .iter()
        .map(|name| format!("{} {order}", quote_ident(name)))
        .collect::<Vec<_>>()
        .join(", ");
    execute_sql(df, &format!("SELECT * FROM df ORDER BY {fields}"))
}

pub fn export_csv(df: &DataFrame, path: &Path) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    let mut output = df.clone();
    CsvWriter::new(&mut file)
        .include_header(true)
        .finish(&mut output)?;
    Ok(())
}

pub fn analyze(df: &DataFrame, columns: &[String]) -> Result<Analysis> {
    if columns.is_empty() {
        bail!("enter one or two comma-separated column names");
    }
    let mut stats = Vec::new();
    for name in columns.iter().take(2) {
        let column = df
            .column(name)
            .map_err(|error| anyhow!("unknown column {name}: {error}"))?;
        let values = numeric_values(column);
        let count = column.len().saturating_sub(column.null_count());
        let unique = column.n_unique().unwrap_or(0);
        let (min, max, mean, std_dev) = if values.is_empty() {
            let mut strings = (0..column.len())
                .filter_map(|idx| column.get(idx).ok())
                .filter(|v| !matches!(v, AnyValue::Null))
                .map(|v| display_value(&v))
                .collect::<Vec<_>>();
            strings.sort();
            (
                strings.first().cloned().unwrap_or_else(|| "—".into()),
                strings.last().cloned().unwrap_or_else(|| "—".into()),
                None,
                None,
            )
        } else {
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance = if values.len() > 1 {
                values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64
            } else {
                0.0
            };
            (fixed2(min), fixed2(max), Some(mean), Some(variance.sqrt()))
        };
        stats.push(ColumnStats {
            name: name.clone(),
            dtype: format!("{:?}", column.dtype()),
            count,
            nulls: column.null_count(),
            unique,
            min,
            max,
            mean,
            std_dev,
        });
    }

    let first_values = numeric_values(df.column(&columns[0])?);
    let histogram = histogram(&first_values, 8);
    let correlation = if columns.len() >= 2 {
        let a = df.column(&columns[0])?;
        let b = df.column(&columns[1])?;
        pearson(a, b).map(|r| (columns[0].clone(), columns[1].clone(), r))
    } else {
        None
    };

    Ok(Analysis {
        stats,
        correlation,
        histogram,
    })
}

fn numeric_values(column: &Column) -> Vec<f64> {
    (0..column.len())
        .filter_map(|idx| column.get(idx).ok().and_then(any_to_f64))
        .filter(|v| v.is_finite())
        .collect()
}

fn pearson(a: &Column, b: &Column) -> Option<f64> {
    let pairs = (0..a.len().min(b.len()))
        .filter_map(|idx| Some((any_to_f64(a.get(idx).ok()?)?, any_to_f64(b.get(idx).ok()?)?)))
        .collect::<Vec<_>>();
    if pairs.len() < 2 {
        return None;
    }
    let mean_a = pairs.iter().map(|p| p.0).sum::<f64>() / pairs.len() as f64;
    let mean_b = pairs.iter().map(|p| p.1).sum::<f64>() / pairs.len() as f64;
    let numerator = pairs
        .iter()
        .map(|p| (p.0 - mean_a) * (p.1 - mean_b))
        .sum::<f64>();
    let da = pairs.iter().map(|p| (p.0 - mean_a).powi(2)).sum::<f64>();
    let db = pairs.iter().map(|p| (p.1 - mean_b).powi(2)).sum::<f64>();
    let denominator = (da * db).sqrt();
    (denominator > 0.0).then_some(numerator / denominator)
}

fn histogram(values: &[f64], bins: usize) -> Vec<HistogramBin> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < f64::EPSILON {
        return vec![HistogramBin {
            label: fixed2(min),
            count: values.len(),
        }];
    }
    let width = (max - min) / bins as f64;
    let mut counts = vec![0usize; bins];
    for value in values {
        let idx = (((value - min) / width).floor() as usize).min(bins - 1);
        counts[idx] += 1;
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(idx, count)| HistogramBin {
            label: format!(
                "{}–{}",
                fixed2(min + idx as f64 * width),
                fixed2(min + (idx + 1) as f64 * width)
            ),
            count,
        })
        .collect()
}

pub fn display_value(value: &AnyValue<'_>) -> String {
    match value {
        AnyValue::Null => "NULL".into(),
        AnyValue::String(v) => (*v).into(),
        AnyValue::StringOwned(v) => v.to_string(),
        AnyValue::UInt8(v) => format!("{v:.2}"),
        AnyValue::UInt16(v) => format!("{v:.2}"),
        AnyValue::UInt32(v) => format!("{v:.2}"),
        AnyValue::UInt64(v) => format!("{v:.2}"),
        AnyValue::UInt128(v) => format!("{v:.2}"),
        AnyValue::Int8(v) => format!("{v:.2}"),
        AnyValue::Int16(v) => format!("{v:.2}"),
        AnyValue::Int32(v) => format!("{v:.2}"),
        AnyValue::Int64(v) => format!("{v:.2}"),
        AnyValue::Int128(v) => format!("{v:.2}"),
        AnyValue::Float16(v) => format!("{:.2}", f32::from(*v)),
        AnyValue::Float32(v) => format!("{v:.2}"),
        AnyValue::Float64(v) => format!("{v:.2}"),
        _ => value.to_string(),
    }
}

pub fn fixed2(value: f64) -> String {
    format!("{value:.2}")
}
#[allow(dead_code)]
pub fn sig3(value: f64) -> String {
    if value == 0.0 {
        return "0.00".into();
    }
    let decimals = (2 - value.abs().log10().floor() as i32).max(0) as usize;
    format!("{value:.decimals$}")
}

fn any_to_f64(value: AnyValue<'_>) -> Option<f64> {
    match value {
        AnyValue::UInt8(v) => Some(v as f64),
        AnyValue::UInt16(v) => Some(v as f64),
        AnyValue::UInt32(v) => Some(v as f64),
        AnyValue::UInt64(v) => Some(v as f64),
        AnyValue::UInt128(v) => Some(v as f64),
        AnyValue::Int8(v) => Some(v as f64),
        AnyValue::Int16(v) => Some(v as f64),
        AnyValue::Int32(v) => Some(v as f64),
        AnyValue::Int64(v) => Some(v as f64),
        AnyValue::Int128(v) => Some(v as f64),
        AnyValue::Float16(v) => Some(f32::from(v) as f64),
        AnyValue::Float32(v) => Some(v as f64),
        AnyValue::Float64(v) => Some(v),
        _ => None,
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_subsequence_works() {
        assert!(fuzzy_match("Engineering", "eng"));
        assert!(fuzzy_match("Sales", "sls"));
        assert!(!fuzzy_match("Sales", "xyz"));
    }

    #[test]
    fn numeric_formats_keep_zeroes() {
        assert_eq!(fixed2(1.0), "1.00");
        assert_eq!(sig3(0.8782), "0.878");
        assert_eq!(sig3(45.55), "45.5");
    }

    #[test]
    fn sql_like_and_analysis_use_real_dataframe() {
        let frame = df!(
            "name" => ["Alice", "Bob", "Carla"],
            "age" => [35, 44, 51],
            "salary" => [12_500.0, 9_800.0, 15_200.0]
        )
        .unwrap();
        let filtered = execute_sql_like(&frame, "select where age > 40").unwrap();
        assert_eq!(filtered.height(), 2);

        let report = analyze(&frame, &["age".to_owned(), "salary".to_owned()]).unwrap();
        assert_eq!(report.stats.len(), 2);
        assert!(report.correlation.is_some());
        assert!(!report.histogram.is_empty());
    }
}
