//! Regression engines: ridge-penalised Poisson GLM (Newton/IRLS), ordinary least squares,
//! Firth-penalised logistic regression, and a Poisson random-intercept model (Laplace).

use nalgebra::{DMatrix, DVector};

use crate::stats::dist;

const MAX_ITERATIONS: usize = 100;
const TOLERANCE: f64 = 1e-10;

/// A Poisson regression problem: `y ~ Poisson(exp(offset + X β))`, maximising the
/// log-likelihood minus `½ Σ penalty_j β_j²`.
#[derive(Debug, Clone)]
pub struct PoissonProblem {
    pub x: DMatrix<f64>,
    pub y: DVector<f64>,
    /// Usually log(exposure), e.g. log of minutes played.
    pub offset: DVector<f64>,
    pub penalty: DVector<f64>,
}

#[derive(Debug, Clone)]
pub struct PoissonFit {
    pub coefficients: DVector<f64>,
    /// From the inverse penalised Hessian (a Bayesian-style standard error under ridge).
    pub standard_errors: DVector<f64>,
    pub fitted: DVector<f64>,
    pub log_likelihood: f64,
    pub deviance: f64,
    /// log det of the penalised Hessian `XᵀWX + Λ` at the optimum.
    pub log_det_hessian: f64,
    pub converged: bool,
}

fn poisson_log_likelihood(y: &DVector<f64>, mu: &DVector<f64>) -> f64 {
    y.iter()
        .zip(mu.iter())
        .map(|(&yi, &mi)| {
            let ln_factorial = statrs::function::gamma::ln_gamma(yi + 1.0);
            if mi <= 0.0 {
                if yi == 0.0 { 0.0 } else { f64::NEG_INFINITY }
            } else {
                yi * mi.ln() - mi - ln_factorial
            }
        })
        .sum()
}

#[must_use]
pub fn poisson_deviance(y: &DVector<f64>, mu: &DVector<f64>) -> f64 {
    2.0 * y
        .iter()
        .zip(mu.iter())
        .map(|(&yi, &mi)| {
            let term = if yi > 0.0 { yi * (yi / mi).ln() } else { 0.0 };
            term - (yi - mi)
        })
        .sum::<f64>()
}

fn means(problem: &PoissonProblem, beta: &DVector<f64>) -> DVector<f64> {
    let eta = &problem.x * beta + &problem.offset;
    eta.map(|e| e.clamp(-50.0, 50.0).exp())
}

fn objective(problem: &PoissonProblem, beta: &DVector<f64>) -> f64 {
    let mu = means(problem, beta);
    let penalty: f64 = beta
        .iter()
        .zip(problem.penalty.iter())
        .map(|(b, l)| 0.5 * l * b * b)
        .sum();
    poisson_log_likelihood(&problem.y, &mu) - penalty
}

fn penalised_hessian(problem: &PoissonProblem, mu: &DVector<f64>) -> DMatrix<f64> {
    let weighted = DMatrix::from_fn(problem.x.nrows(), problem.x.ncols(), |i, j| problem.x[(i, j)] * mu[i]);
    let mut hessian = problem.x.transpose() * weighted;
    for j in 0..hessian.ncols() {
        hessian[(j, j)] += problem.penalty[j];
    }
    hessian
}

/// Fits the penalised Poisson model by damped Newton iterations.
#[must_use]
pub fn fit_poisson(problem: &PoissonProblem) -> Option<PoissonFit> {
    let p = problem.x.ncols();
    let rows = problem.y.len();
    let shapes_match = problem.x.nrows() == rows && problem.offset.len() == rows;
    if !shapes_match || problem.penalty.len() != p {
        return None;
    }
    let mut beta = DVector::zeros(p);
    let mut current = objective(problem, &beta);
    let mut converged = false;
    for _ in 0..MAX_ITERATIONS {
        let mu = means(problem, &beta);
        let gradient = problem.x.transpose() * (&problem.y - &mu)
            - problem.penalty.component_mul(&beta);
        let hessian = penalised_hessian(problem, &mu);
        let step = hessian.clone().cholesky()?.solve(&gradient);
        let mut scale = 1.0;
        let mut improved = false;
        for _ in 0..40 {
            let candidate = &beta + &step * scale;
            let value = objective(problem, &candidate);
            if value >= current - 1e-12 {
                let change = (value - current).abs();
                beta = candidate;
                current = value;
                improved = true;
                if change < TOLERANCE * (1.0 + current.abs()) {
                    converged = true;
                }
                break;
            }
            scale /= 2.0;
        }
        if !improved || converged {
            converged = true;
            break;
        }
    }
    let mu = means(problem, &beta);
    let hessian = penalised_hessian(problem, &mu);
    let cholesky = hessian.cholesky()?;
    let log_det_hessian = 2.0 * cholesky.l().diagonal().iter().map(|d| d.ln()).sum::<f64>();
    let covariance = cholesky.inverse();
    Some(PoissonFit {
        standard_errors: covariance.diagonal().map(|v| v.max(0.0).sqrt()),
        log_likelihood: poisson_log_likelihood(&problem.y, &mu),
        deviance: poisson_deviance(&problem.y, &mu),
        fitted: mu,
        coefficients: beta,
        log_det_hessian,
        converged,
    })
}

#[derive(Debug, Clone)]
pub struct OlsFit {
    pub coefficients: DVector<f64>,
    pub standard_errors: DVector<f64>,
    pub t_values: DVector<f64>,
    pub p_values: DVector<f64>,
    pub r_squared: f64,
    pub adjusted_r_squared: f64,
    pub f_statistic: f64,
    pub f_p_value: f64,
    pub residual_df: f64,
}

/// Least squares with an intercept column already included in `x` (column 0).
#[must_use]
pub fn fit_ols(x: &DMatrix<f64>, y: &DVector<f64>) -> Option<OlsFit> {
    let (n, p) = (x.nrows(), x.ncols());
    if n <= p || y.len() != n {
        return None;
    }
    let xtx = x.transpose() * x;
    let inverse = xtx.try_inverse()?;
    let beta = &inverse * x.transpose() * y;
    let residuals = y - x * &beta;
    let rss = residuals.norm_squared();
    let y_mean = y.mean();
    let tss: f64 = y.iter().map(|v| (v - y_mean).powi(2)).sum();
    let residual_df = (n - p) as f64;
    let sigma2 = rss / residual_df;
    let standard_errors = inverse.diagonal().map(|v| (v * sigma2).max(0.0).sqrt());
    let t_values = beta.component_div(&standard_errors);
    let p_values = t_values.map(|t| dist::t_two_sided_p(t, residual_df).unwrap_or(f64::NAN));
    let r_squared = if tss > 0.0 { 1.0 - rss / tss } else { 0.0 };
    let model_df = (p - 1) as f64;
    let adjusted_r_squared = 1.0 - (1.0 - r_squared) * (n as f64 - 1.0) / residual_df;
    let f_statistic = if model_df > 0.0 && rss > 0.0 {
        ((tss - rss) / model_df) / sigma2
    } else {
        f64::NAN
    };
    let f_p_value = dist::f_sf(f_statistic, model_df, residual_df).unwrap_or(f64::NAN);
    Some(OlsFit {
        coefficients: beta,
        standard_errors,
        t_values,
        p_values,
        r_squared,
        adjusted_r_squared,
        f_statistic,
        f_p_value,
        residual_df,
    })
}

#[derive(Debug, Clone)]
pub struct LogisticFit {
    pub coefficients: DVector<f64>,
    pub standard_errors: DVector<f64>,
    /// Wald p-values from the Firth-penalised fit.
    pub p_values: DVector<f64>,
    pub fitted: DVector<f64>,
    pub converged: bool,
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z.clamp(-40.0, 40.0)).exp())
}

/// Logistic regression with Firth's bias-reducing penalty, which keeps estimates finite
/// under separation and reduces small-sample bias. `x` includes the intercept column.
#[must_use]
pub fn fit_firth_logistic(x: &DMatrix<f64>, y: &DVector<f64>) -> Option<LogisticFit> {
    let (rows, cols) = (x.nrows(), x.ncols());
    if y.len() != rows || rows == 0 {
        return None;
    }
    let mut beta = DVector::zeros(cols);
    let mut converged = false;
    for _ in 0..MAX_ITERATIONS {
        let pi = (x * &beta).map(sigmoid);
        let w = pi.map(|q| (q * (1.0 - q)).max(1e-12));
        let weighted = DMatrix::from_fn(rows, cols, |i, j| x[(i, j)] * w[i]);
        let information = x.transpose() * &weighted;
        let inverse = information.clone().try_inverse()?;
        let leverage = DVector::from_fn(rows, |i, _| {
            let row = x.row(i);
            w[i] * (row * &inverse * row.transpose())[(0, 0)]
        });
        let adjusted = DVector::from_fn(rows, |i, _| y[i] - pi[i] + leverage[i] * (0.5 - pi[i]));
        let score = x.transpose() * adjusted;
        let step = &inverse * &score;
        let max_step = step.amax();
        let scale = if max_step > 5.0 { 5.0 / max_step } else { 1.0 };
        beta += step * scale;
        if max_step * scale < 1e-9 {
            converged = true;
            break;
        }
    }
    let pi = (x * &beta).map(sigmoid);
    let w = pi.map(|q| (q * (1.0 - q)).max(1e-12));
    let weighted = DMatrix::from_fn(rows, cols, |i, j| x[(i, j)] * w[i]);
    let covariance = (x.transpose() * weighted).try_inverse()?;
    let standard_errors = covariance.diagonal().map(|v| v.max(0.0).sqrt());
    let p_values = DVector::from_fn(cols, |j, _| dist::normal_two_sided_p(beta[j] / standard_errors[j]));
    Some(LogisticFit {
        coefficients: beta,
        standard_errors,
        p_values,
        fitted: pi,
        converged,
    })
}

/// Poisson model with fixed effects `x` and one random intercept per group
/// (`group[i]` indexes into `0..groups`), variance chosen by Laplace-approximate marginal
/// likelihood over a grid.
#[derive(Debug, Clone)]
pub struct RandomInterceptFit {
    pub fixed: PoissonFit,
    pub random_effects: Vec<f64>,
    pub variance: f64,
    pub log_marginal: f64,
}

#[must_use]
pub fn fit_poisson_random_intercept(
    x: &DMatrix<f64>,
    y: &DVector<f64>,
    offset: &DVector<f64>,
    group: &[usize],
    groups: usize,
    fixed_penalty: &DVector<f64>,
) -> Option<RandomInterceptFit> {
    let (n, p) = (x.nrows(), x.ncols());
    if group.len() != n || groups == 0 {
        return None;
    }
    let design = DMatrix::from_fn(n, p + groups, |i, j| {
        if j < p {
            x[(i, j)]
        } else if group[i] == j - p {
            1.0
        } else {
            0.0
        }
    });
    let variance_grid = [0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.35, 0.5, 0.75, 1.0, 1.5, 2.0];
    let mut best: Option<RandomInterceptFit> = None;
    for &variance in &variance_grid {
        let penalty = DVector::from_fn(p + groups, |j, _| {
            if j < p { fixed_penalty[j] } else { 1.0 / variance }
        });
        let problem = PoissonProblem {
            x: design.clone(),
            y: y.clone(),
            offset: offset.clone(),
            penalty,
        };
        let Some(fit) = fit_poisson(&problem) else {
            continue;
        };
        let random: Vec<f64> = fit.coefficients.iter().skip(p).copied().collect();
        let mu = &fit.fitted;
        let mut random_block = DMatrix::<f64>::identity(groups, groups);
        for (i, &g) in group.iter().enumerate() {
            random_block[(g, g)] += variance * mu[i];
        }
        let log_det = random_block
            .cholesky()
            .map(|c| 2.0 * c.l().diagonal().iter().map(|d| d.ln()).sum::<f64>())?;
        let prior: f64 = random.iter().map(|u| u * u).sum::<f64>() / (2.0 * variance);
        let log_marginal = fit.log_likelihood - prior - 0.5 * log_det;
        if best.as_ref().is_none_or(|b| log_marginal > b.log_marginal) {
            best = Some(RandomInterceptFit {
                fixed: fit,
                random_effects: random,
                variance,
                log_marginal,
            });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    fn poisson_data() -> (DMatrix<f64>, DVector<f64>, DVector<f64>) {
        let x = DMatrix::from_row_slice(8, 3, &[
            1.0, 0.0, 1.0,
            1.0, 1.0, 0.0,
            1.0, 0.0, 0.0,
            1.0, 1.0, 1.0,
            1.0, 0.0, 1.0,
            1.0, 1.0, 0.0,
            1.0, 1.0, 1.0,
            1.0, 0.0, 0.0,
        ]);
        let y = DVector::from_vec(vec![4.0, 7.0, 2.0, 11.0, 3.0, 5.0, 9.0, 1.0]);
        let exposure = DVector::from_vec(vec![10.0, 12.0, 8.0, 15.0, 9.0, 11.0, 13.0, 7.0]);
        (x, y, exposure.map(f64::ln))
    }

    #[test]
    fn unpenalised_poisson_matches_statsmodels() {
        let (x, y, offset) = poisson_data();
        let fit = fit_poisson(&PoissonProblem {
            x,
            y,
            offset,
            penalty: DVector::zeros(3),
        })
        .unwrap();
        assert!(fit.converged);
        for (got, want) in fit.coefficients.iter().zip(REF_POISSON_COEF) {
            assert!(close(*got, want, 1e-7), "{got} vs {want}");
        }
        for (got, want) in fit.standard_errors.iter().zip(REF_POISSON_SE) {
            assert!(close(*got, want, 1e-7), "{got} vs {want}");
        }
        assert!(close(fit.deviance, REF_POISSON_DEVIANCE, 1e-7));
    }

    #[test]
    fn ridge_shrinks_toward_zero() {
        let (x, y, offset) = poisson_data();
        let loose = fit_poisson(&PoissonProblem { x: x.clone(), y: y.clone(), offset: offset.clone(), penalty: DVector::from_vec(vec![0.0, 0.0, 0.0]) }).unwrap();
        let tight = fit_poisson(&PoissonProblem { x, y, offset, penalty: DVector::from_vec(vec![0.0, 50.0, 50.0]) }).unwrap();
        assert!(tight.coefficients[1].abs() < loose.coefficients[1].abs());
        assert!(tight.coefficients[2].abs() < loose.coefficients[2].abs());
    }

    #[test]
    fn ols_matches_statsmodels() {
        let x = DMatrix::from_row_slice(6, 2, &[1.0, 1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 4.0, 1.0, 5.0, 1.0, 6.0]);
        let y = DVector::from_vec(vec![2.1, 3.9, 6.2, 7.8, 10.1, 12.2]);
        let fit = fit_ols(&x, &y).unwrap();
        assert!(close(fit.coefficients[1], REF_OLS_SLOPE, 1e-9));
        assert!(close(fit.standard_errors[1], REF_OLS_SLOPE_SE, 1e-9));
        assert!(close(fit.r_squared, REF_OLS_R2, 1e-9));
    }

    #[test]
    fn firth_matches_reference_implementation_and_survives_separation() {
        let x = DMatrix::from_row_slice(8, 2, &[1.0, -2.0, 1.0, -1.5, 1.0, -1.0, 1.0, -0.5, 1.0, 0.5, 1.0, 1.0, 1.0, 1.5, 1.0, 2.0]);
        let separated = DVector::from_vec(vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        let fit = fit_firth_logistic(&x, &separated).unwrap();
        assert!(fit.converged);
        assert!(fit.coefficients[1].is_finite());
        assert!(close(fit.coefficients[1], REF_FIRTH_SLOPE, 1e-6), "{}", fit.coefficients[1]);
    }

    #[test]
    fn random_intercepts_recover_group_ordering() {
        let groups = 4;
        let group: Vec<usize> = (0..40).map(|i| i % groups).collect();
        let true_effect: [f64; 4] = [-0.6, -0.2, 0.2, 0.6];
        let offset = DVector::from_element(40, (20.0_f64).ln());
        let y = DVector::from_fn(40, |i, _| (20.0 * (0.1 + true_effect[group[i]]).exp()).round());
        let x = DMatrix::from_element(40, 1, 1.0);
        let fit = fit_poisson_random_intercept(&x, &y, &offset, &group, groups, &DVector::from_element(1, 0.0)).unwrap();
        let u = &fit.random_effects;
        assert!(u[0] < u[1] && u[1] < u[2] && u[2] < u[3], "{u:?}");
    }

    // Reference values computed with statsmodels 0.14 and an independent numpy Firth fit.
    const REF_POISSON_COEF: [f64; 3] = [-1.454_650_247_147_367_2, 0.761_309_120_883_579_7, 0.381_670_284_575_556_74];
    const REF_POISSON_SE: [f64; 3] = [0.379_193_364_925_219_27, 0.362_296_225_297_741_86, 0.322_041_089_158_255_63];
    const REF_POISSON_DEVIANCE: f64 = 0.624_665_248_317_111_8;
    const REF_OLS_SLOPE: f64 = 2.02;
    const REF_OLS_SLOPE_SE: f64 = 0.042_761_798_705_988_1;
    const REF_OLS_R2: f64 = 0.998_210_666_107_499_8;
    const REF_FIRTH_SLOPE: f64 = 1.779_268_752_802_391_8;
}
