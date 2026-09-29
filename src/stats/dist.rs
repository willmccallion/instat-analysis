//! Probability helpers over `statrs` distributions. Invalid parameters yield `None`.

use statrs::distribution::{
    Beta, ChiSquared, ContinuousCDF, FisherSnedecor, Normal, StudentsT,
};
use statrs::function::gamma::ln_gamma;

#[must_use]
pub fn normal_cdf(z: f64) -> f64 {
    Normal::new(0.0, 1.0).map_or(f64::NAN, |n| n.cdf(z))
}

/// Standard normal quantile; `p` must be in (0, 1).
#[must_use]
pub fn normal_quantile(p: f64) -> Option<f64> {
    (p > 0.0 && p < 1.0)
        .then(|| Normal::new(0.0, 1.0).ok().map(|n| n.inverse_cdf(p)))
        .flatten()
}

/// Two-sided p-value for a standard normal statistic.
#[must_use]
pub fn normal_two_sided_p(z: f64) -> f64 {
    2.0 * (1.0 - normal_cdf(z.abs()))
}

#[must_use]
pub fn t_two_sided_p(t: f64, df: f64) -> Option<f64> {
    let dist = StudentsT::new(0.0, 1.0, df).ok()?;
    Some(2.0 * dist.sf(t.abs()))
}

#[must_use]
pub fn t_quantile(p: f64, df: f64) -> Option<f64> {
    let dist = StudentsT::new(0.0, 1.0, df).ok()?;
    (p > 0.0 && p < 1.0).then(|| dist.inverse_cdf(p))
}

/// Upper tail P(X ≥ x) for chi-square with `df` degrees of freedom.
#[must_use]
pub fn chi_square_sf(x: f64, df: f64) -> Option<f64> {
    Some(ChiSquared::new(df).ok()?.sf(x.max(0.0)))
}

/// Upper tail P(F ≥ x) for the F distribution.
#[must_use]
pub fn f_sf(x: f64, df1: f64, df2: f64) -> Option<f64> {
    Some(FisherSnedecor::new(df1, df2).ok()?.sf(x.max(0.0)))
}

#[must_use]
pub fn beta_quantile(p: f64, a: f64, b: f64) -> Option<f64> {
    let dist = Beta::new(a, b).ok()?;
    (p > 0.0 && p < 1.0).then(|| dist.inverse_cdf(p))
}

#[must_use]
pub fn ln_poisson_pmf(k: u64, mu: f64) -> f64 {
    if mu <= 0.0 {
        return if k == 0 { 0.0 } else { f64::NEG_INFINITY };
    }
    let k = k as f64;
    k * mu.ln() - mu - ln_gamma(k + 1.0)
}

/// Exact two-sided Poisson test of observing `k` given expectation `mu`: sums the
/// probability of every outcome no more likely than `k` (the "minlike" method).
#[must_use]
pub fn poisson_exact_two_sided(k: u64, mu: f64) -> f64 {
    if mu <= 0.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    let observed = ln_poisson_pmf(k, mu);
    let tolerance = 1e-7;
    let limit = (mu + 12.0 * mu.sqrt() + 20.0).ceil() as u64 + k;
    let total: f64 = (0..=limit)
        .map(|j| ln_poisson_pmf(j, mu))
        .filter(|lp| *lp <= observed + tolerance)
        .map(f64::exp)
        .sum();
    total.min(1.0)
}

/// Two-sided exact binomial test of `k` successes in `n` trials with probability `p`
/// (minlike method, as R's `binom.test`).
#[must_use]
pub fn binomial_exact_two_sided(k: u64, n: u64, p: f64) -> f64 {
    let ln_pmf = |j: u64| {
        let (jf, nf) = (j as f64, n as f64);
        ln_gamma(nf + 1.0) - ln_gamma(jf + 1.0) - ln_gamma(nf - jf + 1.0)
            + if p > 0.0 { jf * p.ln() } else if j == 0 { 0.0 } else { f64::NEG_INFINITY }
            + if p < 1.0 { (nf - jf) * (1.0 - p).ln() } else if j == n { 0.0 } else { f64::NEG_INFINITY }
    };
    let observed = ln_pmf(k);
    let relative = 1.0_f64 + 1e-7;
    (0..=n)
        .map(ln_pmf)
        .filter(|lp| *lp <= observed + relative.ln())
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

    // Reference values computed with scipy 1.18.
    #[test]
    fn matches_scipy_reference_values() {
        assert!(close(normal_cdf(1.96), 0.975_002_104_851_779_5, 1e-9));
        assert!(close(t_two_sided_p(2.1, 7.0).unwrap(), 0.073_871_196_212_922_65, 1e-8));
        assert!(close(chi_square_sf(7.5, 3.0).unwrap(), 0.057_558_451_972_636_4, 1e-8));
        assert!(close(f_sf(3.2, 2.0, 15.0).unwrap(), 0.069_595_497_352_757_34, 1e-8));
        assert!(close(beta_quantile(0.05, 12.0, 9.0).unwrap(), 0.393_584_886_756_969_57, 1e-7));
    }

    #[test]
    fn exact_tests_match_r() {
        // scipy: sum of poisson.pmf(j, 4.2) <= pmf(9) (R's poisson.test method)
        assert!(close(poisson_exact_two_sided(9, 4.2), 0.042_927_767_778_502, 1e-8));
        // scipy: binomtest(7, 20, 0.5).pvalue
        assert!(close(binomial_exact_two_sided(7, 20, 0.5), 0.263_175_964_355_468_75, 1e-9));
    }
}
