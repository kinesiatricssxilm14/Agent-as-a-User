//! Statistical analysis over numeric columns.
//!
//! Operates on plain `f64` slices extracted from the Polars `DataFrame` so the
//! statistics are straightforward to verify. All formatting rules (two decimal
//! places for correlation, three significant figures for summary statistics)
//! are applied by the caller using the `format` module.

/// Descriptive statistics for a single numeric column.
#[derive(Debug, Clone, Default)]
pub struct NumericSummary {
    /// Non-null value count.
    pub count: usize,
    /// Null / missing value count.
    pub missing: usize,
    pub mean: f64,
    /// Sample standard deviation (n - 1 denominator).
    pub std: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    /// Fisher-Pearson sample skewness.
    pub skewness: f64,
    /// Excess kurtosis.
    pub kurtosis: f64,
}

/// Compute summary statistics for a vector of observed values.
pub fn summarize(values: &[f64], missing: usize) -> NumericSummary {
    let mut s = NumericSummary {
        count: values.len(),
        missing,
        ..Default::default()
    };
    if values.is_empty() {
        s.mean = f64::NAN;
        s.std = f64::NAN;
        s.min = f64::NAN;
        s.q1 = f64::NAN;
        s.median = f64::NAN;
        s.q3 = f64::NAN;
        s.max = f64::NAN;
        s.skewness = f64::NAN;
        s.kurtosis = f64::NAN;
        return s;
    }

    let n = values.len() as f64;
    s.mean = values.iter().sum::<f64>() / n;

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    s.min = sorted[0];
    s.max = sorted[sorted.len() - 1];
    s.q1 = quantile(&sorted, 0.25);
    s.median = quantile(&sorted, 0.5);
    s.q3 = quantile(&sorted, 0.75);

    // Sample variance.
    let ss: f64 = values.iter().map(|x| (x - s.mean) * (x - s.mean)).sum();
    if values.len() > 1 {
        s.std = (ss / (values.len() - 1) as f64).sqrt();
    } else {
        s.std = 0.0;
    }

    // Skewness (Fisher-Pearson, adjusted for sample).
    if values.len() >= 3 && s.std > 0.0 {
        let m3: f64 = values.iter().map(|x| (x - s.mean).powi(3)).sum();
        let denom = (values.len() - 1) as f64 * (values.len() - 2) as f64 * s.std.powi(3);
        s.skewness = n * m3 / denom;
    } else if s.std == 0.0 {
        s.skewness = 0.0;
    } else {
        s.skewness = f64::NAN;
    }

    // Excess kurtosis.
    if values.len() >= 4 && s.std > 0.0 {
        let m4: f64 = values.iter().map(|x| (x - s.mean).powi(4)).sum();
        let n = values.len() as f64;
        let a = (n * (n + 1.0)) / ((n - 1.0) * (n - 2.0) * (n - 3.0));
        let b = m4 / s.std.powi(4);
        let c = 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0));
        s.kurtosis = a * b - c;
    } else if s.std == 0.0 {
        s.kurtosis = f64::NAN;
    } else {
        s.kurtosis = f64::NAN;
    }

    s
}

/// Linear-interpolation quantile (type 7, matching common tools).
fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

/// Pearson correlation between two equally sized slices.
pub fn pearson(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len();
    if n < 2 || ys.len() != n {
        return f64::NAN;
    }
    let mx = xs.iter().sum::<f64>() / n as f64;
    let my = ys.iter().sum::<f64>() / n as f64;
    let mut num = 0.0;
    let mut dx2 = 0.0;
    let mut dy2 = 0.0;
    for i in 0..n {
        let dx = xs[i] - mx;
        let dy = ys[i] - my;
        num += dx * dy;
        dx2 += dx * dx;
        dy2 += dy * dy;
    }
    let denom = (dx2 * dy2).sqrt();
    if denom == 0.0 {
        f64::NAN
    } else {
        num / denom
    }
}

/// Pairwise Pearson correlation on two optional-value columns, using only the
/// rows where both are present.
pub fn pearson_pairwise(xs: &[Option<f64>], ys: &[Option<f64>]) -> f64 {
    if xs.len() != ys.len() {
        return f64::NAN;
    }
    let mut a = Vec::with_capacity(xs.len());
    let mut b = Vec::with_capacity(xs.len());
    for (x, y) in xs.iter().zip(ys.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            a.push(*x);
            b.push(*y);
        }
    }
    pearson(&a, &b)
}

/// A single histogram bin.
#[derive(Debug, Clone)]
pub struct HistogramBin {
    pub start: f64,
    pub end: f64,
    pub count: usize,
}

/// Build a histogram with `bins` equal-width buckets over `[min, max]`.
pub fn histogram(values: &[f64], bins: usize) -> Vec<HistogramBin> {
    let bins = bins.max(1);
    if values.is_empty() {
        return Vec::new();
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for v in values {
        if *v < min {
            min = *v;
        }
        if *v > max {
            max = *v;
        }
    }

    if min == max {
        return vec![HistogramBin {
            start: min,
            end: max,
            count: values.len(),
        }];
    }

    let width = (max - min) / bins as f64;
    let mut out: Vec<HistogramBin> = (0..bins)
        .map(|i| HistogramBin {
            start: min + i as f64 * width,
            end: min + (i + 1) as f64 * width,
            count: 0,
        })
        .collect();

    for v in values {
        let mut idx = (((*v - min) / width).floor() as isize).clamp(0, bins as isize - 1) as usize;
        // Rounding guard: the maximum value must fall into the last bin.
        if *v >= max {
            idx = bins - 1;
        }
        out[idx].count += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_basic() {
        let s = summarize(&[1.0, 2.0, 3.0, 4.0, 5.0], 0);
        assert_eq!(s.count, 5);
        assert_eq!(s.mean, 3.0);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 5.0);
        assert_eq!(s.median, 3.0);
        assert_eq!(s.q1, 2.0);
        assert_eq!(s.q3, 4.0);
    }

    #[test]
    fn pearson_perfect() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![2.0, 4.0, 6.0, 8.0];
        let r = pearson(&x, &y);
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn histogram_counts() {
        let vals = vec![0.0, 0.1, 0.2, 0.5, 0.9, 1.0];
        let bins = histogram(&vals, 2);
        assert_eq!(bins.len(), 2);
        // bin 0 = [0.0, 0.5) -> 0.0, 0.1, 0.2 ; bin 1 = [0.5, 1.0] -> 0.5, 0.9, 1.0
        assert_eq!(bins[0].count, 3);
        assert_eq!(bins[1].count, 3);
    }
}
