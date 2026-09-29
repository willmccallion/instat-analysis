//! Principal components and k-means clustering for player style profiles.

use nalgebra::DMatrix;

use crate::stats::random::{self, Rng};

#[derive(Debug, Clone)]
pub struct Pca {
    /// `loadings[(feature, component)]`.
    pub loadings: DMatrix<f64>,
    /// `scores[(observation, component)]`.
    pub scores: DMatrix<f64>,
    /// Share of total variance per component, descending.
    pub explained: Vec<f64>,
}

/// Standardises each column (z-scores; constant columns become zero).
#[must_use]
pub fn standardise(data: &DMatrix<f64>) -> DMatrix<f64> {
    let n = data.nrows() as f64;
    let mut out = data.clone();
    for mut column in out.column_iter_mut() {
        let mean = column.sum() / n;
        let variance = column.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
        let sd = variance.sqrt();
        for value in column.iter_mut() {
            *value = if sd > 0.0 { (*value - mean) / sd } else { 0.0 };
        }
    }
    out
}

/// PCA of already-standardised data via SVD. Component signs are fixed so each
/// component's largest-magnitude loading is positive.
#[must_use]
pub fn pca(standardised: &DMatrix<f64>) -> Option<Pca> {
    if standardised.nrows() < 2 || standardised.ncols() == 0 {
        return None;
    }
    let svd = standardised.clone().svd(true, true);
    let v_t = svd.v_t?;
    let mut order: Vec<usize> = (0..svd.singular_values.len()).collect();
    order.sort_by(|&a, &b| svd.singular_values[b].total_cmp(&svd.singular_values[a]));
    let total: f64 = svd.singular_values.iter().map(|s| s * s).sum();
    let components = order.len();
    let mut loadings = DMatrix::zeros(standardised.ncols(), components);
    for (k, &source) in order.iter().enumerate() {
        let row = v_t.row(source);
        let largest = row.iter().copied().fold(0.0_f64, |m, v| if v.abs() > m.abs() { v } else { m });
        let sign = if largest < 0.0 { -1.0 } else { 1.0 };
        for feature in 0..standardised.ncols() {
            loadings[(feature, k)] = sign * row[feature];
        }
    }
    let scores = standardised * &loadings;
    let explained = order
        .iter()
        .map(|&i| if total > 0.0 { svd.singular_values[i].powi(2) / total } else { 0.0 })
        .collect();
    Some(Pca {
        loadings,
        scores,
        explained,
    })
}

#[derive(Debug, Clone)]
pub struct KMeans {
    pub assignments: Vec<usize>,
    pub centroids: DMatrix<f64>,
    pub k: usize,
    pub silhouette: f64,
}

fn squared_distance(data: &DMatrix<f64>, i: usize, centroids: &DMatrix<f64>, c: usize) -> f64 {
    data.row(i)
        .iter()
        .zip(centroids.row(c).iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum()
}

fn point_distance(data: &DMatrix<f64>, i: usize, j: usize) -> f64 {
    data.row(i)
        .iter()
        .zip(data.row(j).iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// k-means++ seeding followed by Lloyd iterations; the best of several restarts is kept.
#[must_use]
pub fn kmeans(data: &DMatrix<f64>, k: usize, rng: &mut Rng) -> Option<KMeans> {
    let n = data.nrows();
    if k < 2 || n <= k {
        return None;
    }
    let mut best: Option<(f64, Vec<usize>, DMatrix<f64>)> = None;
    for _ in 0..10 {
        let (inertia, assignments, centroids) = lloyd(data, k, rng);
        if best.as_ref().is_none_or(|(b, _, _)| inertia < *b) {
            best = Some((inertia, assignments, centroids));
        }
    }
    let (_, assignments, centroids) = best?;
    let silhouette = silhouette(data, &assignments, k);
    Some(KMeans {
        assignments,
        centroids,
        k,
        silhouette,
    })
}

fn lloyd(data: &DMatrix<f64>, k: usize, rng: &mut Rng) -> (f64, Vec<usize>, DMatrix<f64>) {
    let (n, d) = (data.nrows(), data.ncols());
    let mut centroids = DMatrix::zeros(k, d);
    centroids.set_row(0, &data.row(random::index(rng, n)));
    for c in 1..k {
        let weights: Vec<f64> = (0..n)
            .map(|i| (0..c).map(|j| squared_distance(data, i, &centroids, j)).fold(f64::INFINITY, f64::min))
            .collect();
        let total: f64 = weights.iter().sum();
        let pick = if total > 0.0 {
            let target = random::uniform(rng) * total;
            let mut running = 0.0;
            weights
                .iter()
                .position(|w| {
                    running += w;
                    running >= target
                })
                .unwrap_or(n - 1)
        } else {
            random::index(rng, n)
        };
        centroids.set_row(c, &data.row(pick));
    }
    let mut assignments = vec![0; n];
    for _ in 0..100 {
        let mut changed = false;
        for (i, slot) in assignments.iter_mut().enumerate() {
            let nearest = (0..k)
                .min_by(|&a, &b| squared_distance(data, i, &centroids, a).total_cmp(&squared_distance(data, i, &centroids, b)))
                .unwrap_or(0);
            if *slot != nearest {
                *slot = nearest;
                changed = true;
            }
        }
        for c in 0..k {
            let members: Vec<usize> = (0..n).filter(|&i| assignments[i] == c).collect();
            if members.is_empty() {
                continue;
            }
            for j in 0..d {
                centroids[(c, j)] = members.iter().map(|&i| data[(i, j)]).sum::<f64>() / members.len() as f64;
            }
        }
        if !changed {
            break;
        }
    }
    let inertia = (0..n).map(|i| squared_distance(data, i, &centroids, assignments[i])).sum();
    (inertia, assignments, centroids)
}

/// Mean silhouette width; singletons score 0 (as scikit-learn).
#[must_use]
pub fn silhouette(data: &DMatrix<f64>, assignments: &[usize], k: usize) -> f64 {
    let n = data.nrows();
    let scores: Vec<f64> = (0..n)
        .map(|i| {
            let own = assignments[i];
            let own_members: Vec<usize> = (0..n).filter(|&j| j != i && assignments[j] == own).collect();
            if own_members.is_empty() {
                return 0.0;
            }
            let a = own_members.iter().map(|&j| point_distance(data, i, j)).sum::<f64>() / own_members.len() as f64;
            let b = (0..k)
                .filter(|&c| c != own)
                .filter_map(|c| {
                    let members: Vec<usize> = (0..n).filter(|&j| assignments[j] == c).collect();
                    (!members.is_empty()).then(|| {
                        members.iter().map(|&j| point_distance(data, i, j)).sum::<f64>() / members.len() as f64
                    })
                })
                .fold(f64::INFINITY, f64::min);
            if b.is_finite() { (b - a) / a.max(b) } else { 0.0 }
        })
        .collect();
    scores.iter().sum::<f64>() / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pca_explained_variance_matches_numpy() {
        let data = DMatrix::from_row_slice(5, 3, &[
            2.0, 1.0, 0.5, 3.0, 2.5, 1.0, 4.0, 3.0, 0.0, 5.0, 4.5, 1.5, 6.0, 5.0, 1.0,
        ]);
        let result = pca(&standardise(&data)).unwrap();
        for (got, want) in result.explained.iter().zip(REF_EXPLAINED) {
            assert!((got - want).abs() < 1e-9, "{got} vs {want}");
        }
    }

    #[test]
    fn kmeans_separates_obvious_groups() {
        let data = DMatrix::from_row_slice(6, 2, &[0.0, 0.0, 0.1, 0.2, 0.2, 0.1, 5.0, 5.0, 5.1, 5.2, 4.9, 5.1]);
        let mut rng = random::seeded(3);
        let result = kmeans(&data, 2, &mut rng).unwrap();
        assert_eq!(result.assignments[0], result.assignments[1]);
        assert_eq!(result.assignments[3], result.assignments[4]);
        assert_ne!(result.assignments[0], result.assignments[3]);
        assert!(result.silhouette > 0.9);
    }

    // numpy: s = svd(zscore(data, ddof=1)); s**2 / sum(s**2)
    const REF_EXPLAINED: [f64; 3] = [0.775_234_185_811_309_3, 0.222_844_483_580_782, 0.001_921_330_607_908_780_3];
}
