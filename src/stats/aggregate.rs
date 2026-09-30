//! Logistic regression learned from group totals instead of individual outcomes.
//!
//! Each group's predicted probabilities should add up to a known total; used to recover how
//! a per-shot chance of scoring varies with location from per-player xG sums.

use nalgebra::{DMatrix, DVector};

const MAX_ITERATIONS: usize = 200;
const TOLERANCE: f64 = 1e-12;
/// Keeps weakly identified slopes finite without visibly moving well-identified ones.
const SLOPE_RIDGE: f64 = 1e-3;
/// Added to each total before weighting, so groups with a total of zero still count.
const WEIGHT_FLOOR: f64 = 0.05;

/// Rows of the design matrix whose probabilities should sum to `total`.
#[derive(Debug, Clone, PartialEq)]
pub struct TotalGroup {
    pub rows: Vec<usize>,
    pub total: f64,
}

#[derive(Debug, Clone)]
pub struct TotalsFit {
    pub coefficients: DVector<f64>,
    pub converged: bool,
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z.clamp(-40.0, 40.0)).exp())
}

/// Each row's fitted probability under `coefficients`.
#[must_use]
pub fn probabilities(x: &DMatrix<f64>, coefficients: &DVector<f64>) -> DVector<f64> {
    (x * coefficients).map(sigmoid)
}

fn objective(x: &DMatrix<f64>, groups: &[TotalGroup], beta: &DVector<f64>) -> f64 {
    let p = probabilities(x, beta);
    let misfit: f64 = groups
        .iter()
        .map(|g| {
            let predicted: f64 = g.rows.iter().map(|&i| p[i]).sum();
            (predicted - g.total).powi(2) / (g.total + WEIGHT_FLOOR)
        })
        .sum();
    misfit + SLOPE_RIDGE * beta.iter().skip(1).map(|b| b * b).sum::<f64>()
}

/// Least-squares fit (weighted like Poisson counts, variance ∝ total) of a logistic model
/// to group totals, by Levenberg–Marquardt. `x` includes the intercept column.
#[must_use]
pub fn fit_logistic_to_totals(x: &DMatrix<f64>, groups: &[TotalGroup]) -> Option<TotalsFit> {
    let (rows, cols) = (x.nrows(), x.ncols());
    let usable = groups.iter().all(|g| g.total.is_finite() && g.total >= 0.0 && g.rows.iter().all(|&i| i < rows));
    let shots: usize = groups.iter().map(|g| g.rows.len()).sum();
    let total: f64 = groups.iter().map(|g| g.total).sum();
    if !usable || groups.len() < cols || shots == 0 || total <= 0.0 {
        return None;
    }
    let mean_rate = (total / shots as f64).clamp(1e-4, 1.0 - 1e-4);
    let mut beta = DVector::zeros(cols);
    beta[0] = (mean_rate / (1.0 - mean_rate)).ln();
    let mut current = objective(x, groups, &beta);
    let mut damping = 1e-3;
    let mut converged = false;
    for _ in 0..MAX_ITERATIONS {
        let p = probabilities(x, &beta);
        let mut normal = DMatrix::<f64>::zeros(cols, cols);
        let mut gradient = DVector::<f64>::zeros(cols);
        for g in groups {
            let weight = 1.0 / (g.total + WEIGHT_FLOOR);
            let predicted: f64 = g.rows.iter().map(|&i| p[i]).sum();
            let jacobian = g
                .rows
                .iter()
                .fold(DVector::<f64>::zeros(cols), |acc, &i| acc + x.row(i).transpose() * (p[i] * (1.0 - p[i])));
            normal += &jacobian * jacobian.transpose() * weight;
            gradient += &jacobian * (weight * (g.total - predicted));
        }
        for j in 1..cols {
            normal[(j, j)] += SLOPE_RIDGE;
            gradient[j] -= SLOPE_RIDGE * beta[j];
        }
        let mut improved = false;
        for _ in 0..30 {
            let mut damped = normal.clone();
            for j in 0..cols {
                damped[(j, j)] += damping * normal[(j, j)].max(1e-9);
            }
            let step = damped.cholesky()?.solve(&gradient);
            let candidate = &beta + &step;
            let value = objective(x, groups, &candidate);
            if value < current {
                let change = current - value;
                beta = candidate;
                current = value;
                damping = (damping / 3.0).max(1e-9);
                improved = true;
                converged = change < TOLERANCE * (1.0 + current);
                break;
            }
            damping *= 4.0;
        }
        if !improved || converged {
            converged = true;
            break;
        }
    }
    Some(TotalsFit { coefficients: beta, converged })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::random;

    /// Rows `[1, u]` with u spread over [-2, 2], grouped `group_size` at a time.
    fn planted(truth: [f64; 2], groups: usize, group_size: usize) -> (DMatrix<f64>, Vec<TotalGroup>) {
        let mut rng = random::seeded(3);
        let n = groups * group_size;
        let x = DMatrix::from_fn(n, 2, |_, j| if j == 0 { 1.0 } else { 4.0 * random::uniform(&mut rng) - 2.0 });
        let p = probabilities(&x, &DVector::from_row_slice(&truth));
        let groups = (0..groups)
            .map(|g| {
                let rows: Vec<usize> = (g * group_size..(g + 1) * group_size).collect();
                let total = rows.iter().map(|&i| p[i]).sum();
                TotalGroup { rows, total }
            })
            .collect();
        (x, groups)
    }

    #[test]
    fn recovers_planted_coefficients_from_exact_totals() {
        let truth = [-2.0, 0.8];
        let (x, groups) = planted(truth, 30, 5);
        let fit = fit_logistic_to_totals(&x, &groups).unwrap();
        assert!(fit.converged);
        assert!((fit.coefficients[0] - truth[0]).abs() < 0.02, "{}", fit.coefficients[0]);
        assert!((fit.coefficients[1] - truth[1]).abs() < 0.02, "{}", fit.coefficients[1]);
    }

    #[test]
    fn fitted_totals_add_up_to_the_grand_total() {
        let (x, groups) = planted([-1.5, -0.6], 12, 4);
        let fit = fit_logistic_to_totals(&x, &groups).unwrap();
        let p = probabilities(&x, &fit.coefficients);
        let fitted: f64 = p.iter().sum();
        let wanted: f64 = groups.iter().map(|g| g.total).sum();
        assert!((fitted - wanted).abs() < 1e-3, "{fitted} vs {wanted}");
    }

    #[test]
    fn refuses_fewer_groups_than_coefficients() {
        let (x, groups) = planted([-1.0, 0.5], 1, 6);
        assert!(fit_logistic_to_totals(&x, &groups).is_none());
    }
}
