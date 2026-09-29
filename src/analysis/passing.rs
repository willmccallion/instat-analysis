//! Passing network: who feeds whom, compared with what each player's overall passing and
//! receiving volume predicts (quasi-independence, since nobody passes to themselves).

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::model::PlayerId;
use crate::stats::dist;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassEdge {
    pub from: PlayerId,
    pub to: PlayerId,
    pub passes: u32,
    pub expected: f64,
    pub lift: f64,
    pub p: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassingReport {
    pub players: Vec<PlayerRef>,
    /// `matrix[from][to]`, indexed like `players`.
    pub matrix: Vec<Vec<u32>>,
    pub expected: Vec<Vec<f64>>,
    pub made: Vec<u32>,
    pub received: Vec<u32>,
    pub edges: Vec<PassEdge>,
    /// Share of passes returned the other way (0 = one-way, 1 = perfectly mutual).
    pub reciprocity: Option<f64>,
    /// G² statistic, degrees of freedom and p for "passing partners are not random".
    pub quasi_independence: Option<(f64, f64, f64)>,
    pub total: u32,
}

/// Lookup helper for other modules.
#[derive(Debug, Clone, Default)]
pub struct PassTotals {
    index: HashMap<PlayerId, usize>,
    matrix: Vec<Vec<u32>>,
    expected: Vec<Vec<f64>>,
}

impl PassTotals {
    #[must_use]
    pub fn between(&self, from: &PlayerId, to: &PlayerId) -> u32 {
        match (self.index.get(from), self.index.get(to)) {
            (Some(&i), Some(&j)) => self.matrix[i][j],
            _ => 0,
        }
    }

    /// Observed / expected passes in both directions combined.
    #[must_use]
    pub fn lift(&self, a: &PlayerId, b: &PlayerId) -> Option<f64> {
        let (&i, &j) = (self.index.get(a)?, self.index.get(b)?);
        let expected = self.expected[i][j] + self.expected[j][i];
        let observed = f64::from(self.matrix[i][j] + self.matrix[j][i]);
        (expected > 0.0).then(|| observed / expected)
    }
}

/// Iterative proportional fitting of a matrix with a structural-zero diagonal to the
/// observed row and column totals.
#[must_use]
pub fn quasi_independent_expected(matrix: &[Vec<u32>]) -> Vec<Vec<f64>> {
    let n = matrix.len();
    let rows: Vec<f64> = matrix.iter().map(|r| r.iter().map(|&v| f64::from(v)).sum()).collect();
    let cols: Vec<f64> = (0..n).map(|j| matrix.iter().map(|r| f64::from(r[j])).sum()).collect();
    let mut fitted: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 0.0 } else { 1.0 }).collect())
        .collect();
    for _ in 0..500 {
        let mut change = 0.0_f64;
        for (i, row) in fitted.iter_mut().enumerate() {
            let sum: f64 = row.iter().sum();
            if sum > 0.0 {
                for value in row.iter_mut() {
                    let updated = *value * rows[i] / sum;
                    change = change.max((updated - *value).abs());
                    *value = updated;
                }
            }
        }
        for j in 0..n {
            let sum: f64 = fitted.iter().map(|r| r[j]).sum();
            if sum > 0.0 {
                for row in &mut fitted {
                    let updated = row[j] * cols[j] / sum;
                    change = change.max((updated - row[j]).abs());
                    row[j] = updated;
                }
            }
        }
        if change < 1e-9 {
            break;
        }
    }
    fitted
}

fn g_squared(matrix: &[Vec<u32>], expected: &[Vec<f64>]) -> f64 {
    let mut g = 0.0;
    for (row, expected_row) in matrix.iter().zip(expected) {
        for (&observed, &e) in row.iter().zip(expected_row) {
            if observed > 0 && e > 0.0 {
                let o = f64::from(observed);
                g += 2.0 * o * (o / e).ln();
            }
        }
    }
    g
}

#[must_use]
pub fn passing(context: &Context<'_>) -> (PassingReport, PassTotals) {
    let mut ids: Vec<PlayerId> = Vec::new();
    let mut totals: HashMap<(PlayerId, PlayerId), u32> = HashMap::new();
    for game in &context.scope {
        let Some(passes) = &game.passes else {
            continue;
        };
        for (i, from) in passes.players.iter().enumerate() {
            if !ids.contains(from) {
                ids.push(from.clone());
            }
            for (j, to) in passes.players.iter().enumerate() {
                if i != j {
                    *totals.entry((from.clone(), to.clone())).or_default() += passes.values[i][j];
                }
            }
        }
    }
    ids.retain(|id| context.roster.contains_key(id));
    ids.sort_by_key(|id| context.roster.get(id).map(|p| p.name.clone()));
    let n = ids.len();
    let matrix: Vec<Vec<u32>> = ids
        .iter()
        .map(|a| ids.iter().map(|b| totals.get(&(a.clone(), b.clone())).copied().unwrap_or(0)).collect())
        .collect();
    let expected = quasi_independent_expected(&matrix);
    let total: u32 = matrix.iter().flatten().sum();
    let mut edges = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i == j || expected[i][j] <= 0.0 {
                continue;
            }
            let observed = matrix[i][j];
            edges.push(PassEdge {
                from: ids[i].clone(),
                to: ids[j].clone(),
                passes: observed,
                expected: expected[i][j],
                lift: f64::from(observed) / expected[i][j],
                p: dist::poisson_exact_two_sided(u64::from(observed), expected[i][j]),
            });
        }
    }
    edges.sort_by(|a, b| b.passes.cmp(&a.passes).then(b.lift.total_cmp(&a.lift)));
    let mutual: u32 = (0..n)
        .flat_map(|i| ((i + 1)..n).map(move |j| (i, j)))
        .map(|(i, j)| 2 * matrix[i][j].min(matrix[j][i]))
        .sum();
    let active_rows = matrix.iter().filter(|r| r.iter().any(|&v| v > 0)).count();
    let df = (n.saturating_sub(1)).pow(2).saturating_sub(n) as f64;
    let quasi_independence = (total >= 20 && active_rows >= 3 && df > 0.0).then(|| {
        let g = g_squared(&matrix, &expected);
        (g, df, dist::chi_square_sf(g, df).unwrap_or(f64::NAN))
    });
    let report = PassingReport {
        players: ids.iter().filter_map(|id| context.roster.get(id).cloned()).collect(),
        made: matrix.iter().map(|r| r.iter().sum()).collect(),
        received: (0..n).map(|j| matrix.iter().map(|r| r[j]).sum()).collect(),
        reciprocity: (total > 0).then(|| f64::from(mutual) / f64::from(total)),
        quasi_independence,
        edges,
        total,
        expected: expected.clone(),
        matrix: matrix.clone(),
    };
    let lookup = PassTotals {
        index: ids.iter().enumerate().map(|(i, id)| (id.clone(), i)).collect(),
        matrix,
        expected,
    };
    (report, lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quasi_independence_preserves_margins_and_zero_diagonal() {
        let matrix = vec![vec![0, 5, 2], vec![3, 0, 4], vec![1, 6, 0]];
        let fitted = quasi_independent_expected(&matrix);
        for i in 0..3 {
            assert!(fitted[i][i].abs() < 1e-12);
            let row: f64 = fitted[i].iter().sum();
            let observed: f64 = matrix[i].iter().map(|&v| f64::from(v)).sum();
            assert!((row - observed).abs() < 1e-6);
        }
        for j in 0..3 {
            let col: f64 = fitted.iter().map(|r| r[j]).sum();
            let observed: f64 = matrix.iter().map(|r| f64::from(r[j])).sum();
            assert!((col - observed).abs() < 1e-6);
        }
    }
}
