//! Descriptive statistics, ranks, correlations and interval estimates.

use crate::stats::dist;

#[must_use]
pub fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Sample standard deviation (n − 1 denominator).
#[must_use]
pub fn sample_sd(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let m = mean(values)?;
    let ss: f64 = values.iter().map(|v| (v - m).powi(2)).sum();
    Some((ss / (values.len() - 1) as f64).sqrt())
}

/// Linear-interpolated quantile (numpy's default), `q` in [0, 1].
#[must_use]
pub fn quantile(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() || !(0.0..=1.0).contains(&q) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = q * (sorted.len() - 1) as f64;
    let lower = position.floor();
    let fraction = position - lower;
    let index = lower as usize;
    let next = sorted.get(index + 1).copied().unwrap_or(sorted[index]);
    Some(sorted[index] + fraction * (next - sorted[index]))
}

/// Share of `values` at or below `value`, as a 0–100 percentile rank.
#[must_use]
pub fn percentile_rank(values: &[f64], value: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let below = values.iter().filter(|v| **v < value).count() as f64;
    let equal = values.iter().filter(|v| **v == value).count() as f64;
    Some(100.0 * (below + 0.5 * equal) / values.len() as f64)
}

/// Wilson score interval for a binomial proportion at confidence `level`.
#[must_use]
pub fn wilson_interval(successes: f64, trials: f64, level: f64) -> Option<(f64, f64)> {
    if trials <= 0.0 {
        return None;
    }
    let z = dist::normal_quantile(1.0 - (1.0 - level) / 2.0)?;
    let p = successes / trials;
    let denominator = 1.0 + z * z / trials;
    let center = (p + z * z / (2.0 * trials)) / denominator;
    let half = z * (p * (1.0 - p) / trials + z * z / (4.0 * trials * trials)).sqrt() / denominator;
    Some(((center - half).max(0.0), (center + half).min(1.0)))
}

/// Average ranks (1-based), ties sharing the mean rank.
#[must_use]
pub fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut result = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start;
        while end + 1 < order.len() && values[order[end + 1]] == values[order[start]] {
            end += 1;
        }
        let rank = (start + end) as f64 / 2.0 + 1.0;
        for &index in &order[start..=end] {
            result[index] = rank;
        }
        start = end + 1;
    }
    result
}

#[must_use]
pub fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() != y.len() || x.len() < 3 {
        return None;
    }
    let (mx, my) = (mean(x)?, mean(y)?);
    let covariance: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let vx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let vy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    (vx > 0.0 && vy > 0.0).then(|| covariance / (vx * vy).sqrt())
}

#[must_use]
pub fn spearman(x: &[f64], y: &[f64]) -> Option<f64> {
    pearson(&ranks(x), &ranks(y))
}

/// Two-sided p-value for a correlation `r` from `n` pairs (t approximation, as scipy).
#[must_use]
pub fn correlation_p(r: f64, n: usize) -> Option<f64> {
    if n < 3 || r.abs() >= 1.0 {
        return (r.abs() >= 1.0 && n >= 3).then_some(0.0);
    }
    let df = n as f64 - 2.0;
    let t = r * (df / (1.0 - r * r)).sqrt();
    dist::t_two_sided_p(t, df)
}

/// Benjamini–Hochberg adjusted p-values, in the input order.
#[must_use]
pub fn benjamini_hochberg(p_values: &[f64]) -> Vec<f64> {
    let m = p_values.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p_values[b].total_cmp(&p_values[a]));
    let mut adjusted = vec![0.0; m];
    let mut running_min = 1.0_f64;
    for (position, &index) in order.iter().enumerate() {
        let rank = (m - position) as f64;
        running_min = running_min.min(p_values[index] * m as f64 / rank);
        adjusted[index] = running_min.min(1.0);
    }
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn ranks_average_ties() {
        assert_eq!(ranks(&[10.0, 20.0, 10.0, 30.0]), vec![1.5, 3.0, 1.5, 4.0]);
    }

    #[test]
    fn quantile_interpolates_like_numpy() {
        assert!(close(quantile(&[1.0, 2.0, 3.0, 4.0], 0.25).unwrap(), 1.75));
    }

    #[test]
    fn benjamini_hochberg_matches_statsmodels() {
        let adjusted = benjamini_hochberg(&[0.01, 0.04, 0.03, 0.2]);
        let expected = [0.04, 0.053_333_333_333_333_33, 0.053_333_333_333_333_33, 0.2];
        for (a, e) in adjusted.iter().zip(expected) {
            assert!(close(*a, e), "{a} vs {e}");
        }
    }

    #[test]
    fn wilson_matches_statsmodels() {
        let (lo, hi) = wilson_interval(7.0, 20.0, 0.95).unwrap();
        assert!((lo - 0.181_191_824_101_082_03).abs() < 1e-9, "{lo}");
        assert!((hi - 0.567_145_723_314_763_8).abs() < 1e-9, "{hi}");
    }

    #[test]
    fn spearman_matches_scipy() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y = [2.0, 1.0, 4.0, 3.0, 6.0, 5.0];
        let r = spearman(&x, &y).unwrap();
        assert!(close(r, 0.828_571_428_571_428_6), "{r}");
        assert!((correlation_p(r, 6).unwrap() - 0.041_562_682_215_743_35).abs() < 1e-9);
    }
}
