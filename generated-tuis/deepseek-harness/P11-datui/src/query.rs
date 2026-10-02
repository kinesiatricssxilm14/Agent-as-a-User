//! Query execution for the three query modes.
//!
//! * `fuzzy`      – case-insensitive substring match across every column.
//! * `sql_like`   – `select where <condition>` filtered through the SQL engine.
//! * `sql`        – full `select ... from df where ...` executed by Polars'
//!                  built-in SQL context.

use polars::prelude::*;
use polars::sql::SQLContext;

/// Case-insensitive substring search across all columns of a DataFrame.
pub fn fuzzy(df: &DataFrame, query: &str) -> Result<DataFrame, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(df.clone());
    }
    let ql = q.to_lowercase();
    let n = df.height();
    let mut mask = vec![false; n];

    for name in df.get_column_names() {
        let col = df.column(name.as_str()).map_err(|e| e.to_string())?;
        let s_str = col.cast(&DataType::String).map_err(|e| e.to_string())?;
        let ca = s_str.str().map_err(|e| e.to_string())?;
        for (i, v) in ca.iter().enumerate() {
            if let Some(v) = v {
                if v.to_lowercase().contains(&ql) {
                    mask[i] = true;
                }
            }
        }
    }

    let mask_ca = BooleanChunked::from_iter_values("mask".into(), mask.into_iter());
    df.filter(&mask_ca).map_err(|e| e.to_string())
}

/// `select where <condition>` style filtering. Also tolerates a bare
/// `<condition>` (without the `select where` prefix).
pub fn sql_like(df: &DataFrame, query: &str) -> Result<DataFrame, String> {
    let t = query.trim();
    if t.is_empty() {
        return Err("Enter a condition, e.g. `select where age > 40`".to_string());
    }

    let cond = match find_word_ci(t, "where") {
        Some(pos) => t[pos + "where".len()..].trim().to_string(),
        None => {
            if find_word_ci(t, "select").is_some() {
                return Err("Expected `select where <condition>`".to_string());
            }
            t.to_string()
        }
    };

    if cond.is_empty() {
        return Err("Empty condition after `where`".to_string());
    }

    run_sql(df, &format!("SELECT * FROM df WHERE {cond}"))
}

/// Full standard SQL executed against the `df` table.
pub fn sql(df: &DataFrame, query: &str) -> Result<DataFrame, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Enter a SQL query, e.g. `select * from df where age > 40`".to_string());
    }
    run_sql(df, q)
}

fn run_sql(df: &DataFrame, query: &str) -> Result<DataFrame, String> {
    let mut ctx = SQLContext::new();
    let lf = df.clone().lazy();
    ctx.register("df", lf.clone());

    // Register the table under any additional name referenced in the FROM
    // clause so queries such as `select * from employees where ...` also work.
    let table = extract_table_name(query);
    if !table.is_empty() && table != "df" {
        ctx.register(&table, lf);
    }

    let out = ctx.execute(query).map_err(|e| format!("SQL error: {e}"))?;
    out.collect().map_err(|e| format!("SQL error: {e}"))
}

/// Find the first case-insensitive occurrence of `word` as a whole word
/// (not part of a larger identifier). Returns the byte offset into `hay`.
fn find_word_ci(hay: &str, word: &str) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let hb = hay.as_bytes();
    let wb = word.as_bytes();
    if wb.len() > hb.len() {
        return None;
    }
    for i in 0..=(hb.len() - wb.len()) {
        if hb[i..i + wb.len()].eq_ignore_ascii_case(wb) {
            let before_ok = i == 0 || !(hb[i - 1].is_ascii_alphanumeric() || hb[i - 1] == b'_');
            let after = i + wb.len();
            let after_ok = after >= hb.len() || !(hb[after].is_ascii_alphanumeric() || hb[after] == b'_');
            if before_ok && after_ok {
                return Some(i);
            }
        }
    }
    None
}

/// Extract the table name referenced after the first `FROM` keyword.
fn extract_table_name(query: &str) -> String {
    match find_word_ci(query, "from") {
        Some(pos) => {
            let after = &query[pos + "from".len()..];
            let rest = after.trim_start();
            if let Some(stripped) = rest.strip_prefix('"') {
                if let Some(end) = stripped.find('"') {
                    return stripped[..end].to_string();
                }
                return "df".to_string();
            }
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if ident.is_empty() {
                "df".to_string()
            } else {
                ident
            }
        }
        None => "df".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\
id,name,department,age,salary,country,score\n\
1,Alice,Engineering,45,12000.50,US,92.3\n\
2,Bob,Sales,38,8500.00,UK,78.0\n\
3,Carol,Engineering,52,15000.00,US,88.9\n\
4,Dan,Sales,29,6200.75,Canada,65.5\n\
5,Eve,Marketing,41,9300.00,US,71.2\n";

    fn load(csv: &str, tag: &str) -> DataFrame {
        let path = std::env::temp_dir().join(format!(
            "toolk_query_test_{}_{}.csv",
            std::process::id(),
            tag
        ));
        std::fs::write(&path, csv).unwrap();
        let lf = LazyCsvReader::new(path.to_str().unwrap().into())
            .with_has_header(true)
            .finish()
            .unwrap();
        let df = lf.collect().unwrap();
        let _ = std::fs::remove_file(&path);
        df
    }

    #[test]
    fn word_find_ci_whole_word() {
        assert_eq!(find_word_ci("select where age > 40", "where"), Some(7));
        assert_eq!(find_word_ci("SELECT WHERE x", "where"), Some(7));
        assert_eq!(find_word_ci("somewhere else", "where"), None);
        assert_eq!(find_word_ci("no keyword", "where"), None);
    }

    #[test]
    fn table_name_extraction() {
        assert_eq!(extract_table_name("select * from df where a > 1"), "df");
        assert_eq!(extract_table_name("SELECT a FROM employees WHERE x"), "employees");
        assert_eq!(extract_table_name("select * from \"my table\" where x"), "my table");
    }

    #[test]
    fn fuzzy_matches_across_columns() {
        let df = load(CSV, "fuzzy");
        let out = fuzzy(&df, "Sales").unwrap();
        assert_eq!(out.height(), 2); // Bob, Dan
        let out = fuzzy(&df, "engineering").unwrap();
        assert_eq!(out.height(), 2); // Alice, Carol (case-insensitive)
        let out = fuzzy(&df, "85").unwrap();
        assert_eq!(out.height(), 1); // Bob salary 8500.00 -> "8500.00"
    }

    #[test]
    fn sql_like_filters() {
        let df = load(CSV, "sqllike");
        let out = sql_like(&df, "select where age > 40").unwrap();
        assert_eq!(out.height(), 3); // Alice, Carol, Eve
        let out = sql_like(&df, "select where department = 'Engineering' and salary > 10000").unwrap();
        assert_eq!(out.height(), 2); // Alice, Carol
    }

    #[test]
    fn sql_filters() {
        let df = load(CSV, "sql");
        let out = sql(&df, "select * from df where country = 'US' and score > 80").unwrap();
        assert_eq!(out.height(), 2); // Alice (92.3), Carol (88.9)
        let out = sql(&df, "select * from df where age = 35 and salary > 5000 and salary < 15000").unwrap();
        assert_eq!(out.height(), 0);
    }
}
