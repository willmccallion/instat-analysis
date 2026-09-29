//! Classical tests: paired t, Wilcoxon signed-rank, Welch t, chi-square, Fisher exact.

use crate::stats::describe::{mean, ranks, sample_sd};
use crate::stats::dist;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TTest {
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub mean_difference: f64,
    /// 95% confidence interval for the mean difference.
    pub ci: (f64, f64),
    /// Standardised effect (Cohen's d_z for paired data, d for independent).
    pub effect_size: f64,
}

/// Paired t-test of `x − y`. Needs at least two pairs and non-zero spread.
#[must_use]
pub fn paired_t(x: &[f64], y: &[f64]) -> Option<TTest> {
    if x.len() != y.len() {
        return None;
    }
    let diffs: Vec<f64> = x.iter().zip(y).map(|(a, b)| a - b).collect();
    one_sample_t(&diffs)
}

#[must_use]
pub fn one_sample_t(values: &[f64]) -> Option<TTest> {
    let n = values.len() as f64;
    let m = mean(values)?;
    let sd = sample_sd(values)?;
    if sd <= 0.0 {
        return None;
    }
    let se = sd / n.sqrt();
    let df = n - 1.0;
    let t = m / se;
    let critical = dist::t_quantile(0.975, df)?;
    Some(TTest {
        t,
        df,
        p: dist::t_two_sided_p(t, df)?,
        mean_difference: m,
        ci: (m - critical * se, m + critical * se),
        effect_size: m / sd,
    })
}

/// Welch's unequal-variance t-test of mean(x) − mean(y).
#[must_use]
pub fn welch_t(x: &[f64], y: &[f64]) -> Option<TTest> {
    let (nx, ny) = (x.len() as f64, y.len() as f64);
    let (mx, my) = (mean(x)?, mean(y)?);
    let (sx, sy) = (sample_sd(x)?, sample_sd(y)?);
    let (vx, vy) = (sx * sx / nx, sy * sy / ny);
    let se = (vx + vy).sqrt();
    if se <= 0.0 {
        return None;
    }
    let df = (vx + vy).powi(2) / (vx * vx / (nx - 1.0) + vy * vy / (ny - 1.0));
    let t = (mx - my) / se;
    let critical = dist::t_quantile(0.975, df)?;
    let pooled = (((nx - 1.0) * sx * sx + (ny - 1.0) * sy * sy) / (nx + ny - 2.0)).sqrt();
    Some(TTest {
        t,
        df,
        p: dist::t_two_sided_p(t, df)?,
        mean_difference: mx - my,
        ci: (mx - my - critical * se, mx - my + critical * se),
        effect_size: if pooled > 0.0 { (mx - my) / pooled } else { 0.0 },
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wilcoxon {
    /// The smaller of the positive and negative rank sums (scipy's two-sided statistic).
    pub statistic: f64,
    pub p: f64,
    pub n: usize,
    pub exact: bool,
}

/// Wilcoxon signed-rank test on paired differences; zero differences are dropped. Uses the
/// exact null distribution for up to 50 differences when none were zero, else the normal
/// approximation with tie correction (matching scipy's `method="auto"`).
#[must_use]
pub fn wilcoxon_signed_rank(differences: &[f64]) -> Option<Wilcoxon> {
    let nonzero: Vec<f64> = differences.iter().copied().filter(|d| *d != 0.0).collect();
    let n = nonzero.len();
    if n == 0 {
        return None;
    }
    let magnitudes: Vec<f64> = nonzero.iter().map(|d| d.abs()).collect();
    let r = ranks(&magnitudes);
    let positive: f64 = nonzero.iter().zip(&r).filter(|(d, _)| **d > 0.0).map(|(_, rank)| rank).sum();
    let total = (n * (n + 1)) as f64 / 2.0;
    let negative = total - positive;
    let statistic = positive.min(negative);
    let has_zeros = n < differences.len();
    if n <= 50 && !has_zeros {
        let p = exact_signed_rank_p(n, statistic);
        return Some(Wilcoxon {
            statistic,
            p,
            n,
            exact: true,
        });
    }
    let mean_w = total / 2.0;
    let tie_term: f64 = tie_groups(&magnitudes)
        .iter()
        .map(|&t| (t * t * t - t) / 48.0)
        .sum();
    let variance = (n * (n + 1) * (2 * n + 1)) as f64 / 24.0 - tie_term;
    if variance <= 0.0 {
        return None;
    }
    let z = (statistic - mean_w) / variance.sqrt();
    Some(Wilcoxon {
        statistic,
        p: dist::normal_two_sided_p(z),
        n,
        exact: false,
    })
}

fn tie_groups(values: &[f64]) -> Vec<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut groups = Vec::new();
    let mut run = 1.0;
    for window in sorted.windows(2) {
        if window[0] == window[1] {
            run += 1.0;
        } else {
            if run > 1.0 {
                groups.push(run);
            }
            run = 1.0;
        }
    }
    if run > 1.0 {
        groups.push(run);
    }
    groups
}

/// Two-sided exact p-value: 2 · P(W ≤ w) under the signed-rank null, capped at 1.
fn exact_signed_rank_p(n: usize, w: f64) -> f64 {
    let max_sum = n * (n + 1) / 2;
    let mut counts = vec![0.0_f64; max_sum + 1];
    counts[0] = 1.0;
    for k in 1..=n {
        for s in (k..=max_sum).rev() {
            counts[s] += counts[s - k];
        }
    }
    let total: f64 = counts.iter().sum();
    let cutoff = w.floor() as usize;
    let lower: f64 = counts.iter().take(cutoff + 1).sum();
    (2.0 * lower / total).min(1.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChiSquare {
    pub statistic: f64,
    pub df: f64,
    pub p: f64,
    pub min_expected: f64,
    pub cramers_v: f64,
    /// Yates' continuity correction applied (2×2 tables, as scipy).
    pub corrected: bool,
}

/// Chi-square test of independence on an r×c table of counts.
#[must_use]
pub fn chi_square_independence(table: &[Vec<f64>]) -> Option<ChiSquare> {
    let rows = table.len();
    let cols = table.first()?.len();
    if rows < 2 || cols < 2 || table.iter().any(|r| r.len() != cols) {
        return None;
    }
    let row_totals: Vec<f64> = table.iter().map(|r| r.iter().sum()).collect();
    let col_totals: Vec<f64> = (0..cols).map(|c| table.iter().map(|r| r[c]).sum()).collect();
    let total: f64 = row_totals.iter().sum();
    if total <= 0.0 || row_totals.contains(&0.0) || col_totals.contains(&0.0) {
        return None;
    }
    let df = ((rows - 1) * (cols - 1)) as f64;
    let corrected = rows == 2 && cols == 2;
    let mut statistic = 0.0;
    let mut min_expected = f64::INFINITY;
    for (r, row) in table.iter().enumerate() {
        for (c, observed) in row.iter().enumerate() {
            let expected = row_totals[r] * col_totals[c] / total;
            min_expected = min_expected.min(expected);
            let mut deviation = (observed - expected).abs();
            if corrected {
                deviation = (deviation - 0.5).max(0.0);
            }
            statistic += deviation * deviation / expected;
        }
    }
    let smaller = (rows.min(cols) - 1) as f64;
    Some(ChiSquare {
        statistic,
        df,
        p: dist::chi_square_sf(statistic, df)?,
        min_expected,
        cramers_v: (statistic / (total * smaller)).sqrt(),
        corrected,
    })
}

/// Two-sided Fisher exact test for a 2×2 table `[[a, b], [c, d]]`.
#[must_use]
pub fn fisher_exact(a: u64, b: u64, c: u64, d: u64) -> f64 {
    use statrs::function::gamma::ln_gamma;
    let ln_factorial = |n: u64| ln_gamma(n as f64 + 1.0);
    let (row1, row2, col1) = (a + b, c + d, a + c);
    let n = row1 + row2;
    let ln_p = |x: u64| {
        ln_factorial(row1) + ln_factorial(row2) + ln_factorial(col1) + ln_factorial(n - col1)
            - ln_factorial(n)
            - ln_factorial(x)
            - ln_factorial(row1 - x)
            - ln_factorial(col1 - x)
            - ln_factorial(row2 + x - col1)
    };
    let low = col1.saturating_sub(row2);
    let high = row1.min(col1);
    let observed = ln_p(a);
    let relative = (1.0 + 1e-7_f64).ln();
    (low..=high)
        .map(ln_p)
        .filter(|lp| *lp <= observed + relative)
        .map(f64::exp)
        .sum::<f64>()
        .min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    const X: [f64; 7] = [52.0, 48.0, 61.0, 45.0, 57.0, 50.0, 49.0];
    const Y: [f64; 7] = [47.0, 49.0, 52.0, 44.0, 50.0, 51.0, 43.0];

    #[test]
    fn paired_t_matches_scipy() {
        let result = paired_t(&X, &Y).unwrap();
        assert!(close(result.t, REF_PAIRED_T, 1e-9), "{}", result.t);
        assert!(close(result.p, REF_PAIRED_P, 1e-9), "{}", result.p);
    }

    #[test]
    fn welch_matches_scipy() {
        let result = welch_t(&X, &Y).unwrap();
        assert!(close(result.t, REF_WELCH_T, 1e-9), "{}", result.t);
        assert!(close(result.p, REF_WELCH_P, 1e-9), "{}", result.p);
    }

    #[test]
    fn wilcoxon_exact_matches_scipy() {
        let diffs: Vec<f64> = [2.5, -1.0, 9.0, 1.5, 7.0, -3.5, 6.0].to_vec();
        let result = wilcoxon_signed_rank(&diffs).unwrap();
        assert!(result.exact);
        assert!(close(result.statistic, 5.0, 1e-12));
        assert!(close(result.p, REF_WILCOXON_EXACT_P, 1e-9), "{}", result.p);
    }

    #[test]
    fn wilcoxon_with_ties_still_uses_exact_distribution_like_scipy() {
        let result = wilcoxon_signed_rank(&[5.0, 5.0, -2.0, 3.0, 3.0, 1.0, 4.0]).unwrap();
        assert!(result.exact);
        assert!(close(result.p, REF_WILCOXON_APPROX_P, 1e-9), "{}", result.p);
    }

    #[test]
    fn chi_square_matches_scipy() {
        let table = vec![vec![16.0, 18.0, 8.0], vec![14.0, 21.0, 10.0]];
        let result = chi_square_independence(&table).unwrap();
        assert!(close(result.statistic, REF_CHI2_STAT, 1e-9), "{}", result.statistic);
        assert!(close(result.p, REF_CHI2_P, 1e-9));
        let two_by_two = vec![vec![12.0, 5.0], vec![6.0, 14.0]];
        let yates = chi_square_independence(&two_by_two).unwrap();
        assert!(close(yates.statistic, REF_YATES_STAT, 1e-9), "{}", yates.statistic);
    }

    #[test]
    fn fisher_matches_scipy() {
        assert!(close(fisher_exact(3, 1, 1, 3), REF_FISHER_P, 1e-9));
    }

    // Reference values computed with scipy 1.18.
    const REF_PAIRED_T: f64 = 2.438_691_057_974_817_3;
    const REF_PAIRED_P: f64 = 0.050_561_548_524_265_12;
    const REF_WELCH_T: f64 = 1.506_139_560_545_080_2;
    const REF_WELCH_P: f64 = 0.162_704_836_453_559_1;
    const REF_WILCOXON_EXACT_P: f64 = 0.156_25;
    const REF_WILCOXON_APPROX_P: f64 = 0.046_875;
    const REF_CHI2_STAT: f64 = 0.483_451_363_451_363_75;
    const REF_CHI2_P: f64 = 0.785_271_562_348_340_4;
    const REF_YATES_STAT: f64 = 4.543_939_198_486_413;
    const REF_FISHER_P: f64 = 0.485_714_285_714_285_65;
}
