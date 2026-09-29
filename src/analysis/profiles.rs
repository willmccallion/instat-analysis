//! Player style profiles (PCA + k-means on per-60 rates) and how stats relate to results.

use nalgebra::DMatrix;
use serde::Serialize;

use crate::analysis::common::{PlayerRef, TestRow, Verdict};
use crate::analysis::players::PlayerSeason;
use crate::stats::cluster::{kmeans, pca, standardise};
use crate::stats::describe::{correlation_p, spearman};
use crate::stats::random;

const MIN_PLAYERS: usize = 6;

type Feature = (&'static str, &'static str, fn(&PlayerSeason) -> Option<f64>);

/// (label, style word, extractor) — style words name the clusters.
const FEATURES: [Feature; 9] = [
    ("Shots/60", "Shooter", |p| p.rates.shots),
    ("xG/60", "Scoring chances", |p| p.rates.xg),
    ("Passes/60", "Playmaker", |p| p.rates.passes),
    ("Entries/60", "Puck carrier", |p| p.rates.entries),
    ("Recoveries/60", "Puck hound", |p| p.rates.recoveries),
    ("Puck losses/60", "High-risk", |p| p.rates.losses),
    ("Battles won/60", "Board battler", |p| p.rates.battles_won),
    ("Blocks/60", "Shot blocker", |p| p.rates.blocks),
    ("Hits/60", "Physical", |p| p.rates.hits),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfilePoint {
    pub player: PlayerRef,
    pub x: f64,
    pub y: f64,
    pub cluster: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cluster {
    pub label: String,
    pub size: usize,
    /// Feature z-scores of the cluster centre.
    pub centre: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Correlation {
    pub a: String,
    pub b: String,
    pub rho: f64,
    pub p: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfilesReport {
    pub ready: bool,
    pub needs: String,
    pub features: Vec<String>,
    pub explained: Vec<f64>,
    /// `loadings[feature] = (PC1, PC2)`.
    pub loadings: Vec<(f64, f64)>,
    pub points: Vec<ProfilePoint>,
    pub clusters: Vec<Cluster>,
    pub silhouette: Option<f64>,
    pub correlation_labels: Vec<String>,
    /// Spearman ρ matrix over `correlation_labels`.
    pub correlations: Vec<Vec<Option<f64>>>,
    pub with_results: Vec<Correlation>,
}

fn cluster_label(centre: &[f64]) -> String {
    let mut order: Vec<usize> = (0..centre.len()).collect();
    order.sort_by(|&a, &b| centre[b].total_cmp(&centre[a]));
    let top: Vec<&str> = order
        .iter()
        .take(2)
        .filter(|&&i| centre[i] > 0.3)
        .map(|&i| FEATURES[i].1)
        .collect();
    if top.is_empty() {
        "Steady / low-event".to_owned()
    } else {
        top.join(" + ")
    }
}

type Outcome = (&'static str, fn(&PlayerSeason) -> Option<f64>);

const OUTCOMES: [Outcome; 3] = [
    ("On-ice CF%", |p| p.shares.corsi_pct),
    ("On-ice goal share", |p| p.shares.goals_pct),
    ("InStat Index", |p| p.instat_mean),
];

/// Spearman correlations of each style feature with each outcome, added as test rows.
fn correlation_tests(columns: &[Vec<Option<f64>>], tests: &mut Vec<TestRow>) -> Vec<Correlation> {
    let paired = |a: &[Option<f64>], b: &[Option<f64>]| -> (Vec<f64>, Vec<f64>) {
        a.iter().zip(b).filter_map(|(x, y)| Some(((*x)?, (*y)?))).unzip()
    };
    let mut with_results = Vec::new();
    for (o, (outcome, _)) in OUTCOMES.iter().enumerate() {
        for (f, (feature, _, _)) in FEATURES.iter().enumerate() {
            let (x, y) = paired(&columns[f], &columns[FEATURES.len() + o]);
            if let Some(rho) = spearman(&x, &y) {
                let p = correlation_p(rho, x.len());
                with_results.push(Correlation {
                    a: (*feature).to_owned(),
                    b: (*outcome).to_owned(),
                    rho,
                    p,
                });
                tests.push(TestRow {
                    family: "What goes with good results".to_owned(),
                    question: format!("Do players with more {feature} have a better {outcome}?"),
                    method: "Spearman rank correlation across qualified skaters".to_owned(),
                    statistic_label: "ρ".to_owned(),
                    statistic: Some(rho),
                    df: Some(format!("{}", x.len().saturating_sub(2))),
                    p,
                    p_adjusted: None,
                    effect_label: "ρ".to_owned(),
                    effect: Some(rho),
                    ci: None,
                    n: format!("{} players", x.len()),
                    secondary: None,
                    verdict: Verdict::from_adjusted_p(p),
                    plain: format!(
                        "{} relationship (ρ = {rho:.2}) between {feature} and {outcome}.",
                        if rho.abs() >= 0.5 { "Strong" } else if rho.abs() >= 0.3 { "Moderate" } else { "Weak" }
                    ),
                    assumptions: "Players are not independent (they share the ice); read as descriptive.".to_owned(),
                });
            }
        }
    }
    with_results
}

#[must_use]
pub fn profiles(players: &[PlayerSeason], tests: &mut Vec<TestRow>) -> ProfilesReport {
    let qualified: Vec<&PlayerSeason> = players
        .iter()
        .filter(|p| p.qualified && FEATURES.iter().all(|(_, _, f)| f(p).is_some()))
        .collect();
    let labels: Vec<String> = FEATURES
        .iter()
        .map(|(l, _, _)| (*l).to_owned())
        .chain(OUTCOMES.iter().map(|(l, _)| (*l).to_owned()))
        .collect();
    let column = |i: usize| -> Vec<Option<f64>> {
        qualified
            .iter()
            .map(|p| if i < FEATURES.len() { (FEATURES[i].2)(p) } else { (OUTCOMES[i - FEATURES.len()].1)(p) })
            .collect()
    };
    let paired = |a: &[Option<f64>], b: &[Option<f64>]| -> (Vec<f64>, Vec<f64>) {
        a.iter().zip(b).filter_map(|(x, y)| Some(((*x)?, (*y)?))).unzip()
    };
    let columns: Vec<Vec<Option<f64>>> = (0..labels.len()).map(column).collect();
    let correlations: Vec<Vec<Option<f64>>> = columns
        .iter()
        .map(|a| {
            columns
                .iter()
                .map(|b| {
                    let (x, y) = paired(a, b);
                    spearman(&x, &y)
                })
                .collect()
        })
        .collect();
    let with_results = correlation_tests(&columns, tests);
    let base = ProfilesReport {
        ready: false,
        needs: format!("{MIN_PLAYERS} players above the minimum ice time"),
        features: FEATURES.iter().map(|(l, _, _)| (*l).to_owned()).collect(),
        explained: Vec::new(),
        loadings: Vec::new(),
        points: Vec::new(),
        clusters: Vec::new(),
        silhouette: None,
        correlation_labels: labels,
        correlations,
        with_results,
    };
    if qualified.len() < MIN_PLAYERS {
        return base;
    }
    let data = DMatrix::from_fn(qualified.len(), FEATURES.len(), |i, j| (FEATURES[j].2)(qualified[i]).unwrap_or_default());
    let z = standardise(&data);
    let Some(components) = pca(&z) else {
        return base;
    };
    let mut rng = random::seeded(99);
    let max_k = 4.min(qualified.len() - 2);
    let best = (2..=max_k)
        .filter_map(|k| kmeans(&z, k, &mut rng))
        .max_by(|a, b| a.silhouette.total_cmp(&b.silhouette));
    let clusters = best
        .as_ref()
        .map(|km| {
            (0..km.k)
                .map(|c| {
                    let centre: Vec<f64> = km.centroids.row(c).iter().copied().collect();
                    Cluster {
                        label: cluster_label(&centre),
                        size: km.assignments.iter().filter(|&&a| a == c).count(),
                        centre,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let points = qualified
        .iter()
        .enumerate()
        .map(|(i, p)| ProfilePoint {
            player: p.player.clone(),
            x: components.scores[(i, 0)],
            y: if components.scores.ncols() > 1 { components.scores[(i, 1)] } else { 0.0 },
            cluster: best.as_ref().map_or(0, |km| km.assignments[i]),
        })
        .collect();
    ProfilesReport {
        ready: true,
        explained: components.explained.clone(),
        loadings: (0..FEATURES.len())
            .map(|f| (components.loadings[(f, 0)], if components.loadings.ncols() > 1 { components.loadings[(f, 1)] } else { 0.0 }))
            .collect(),
        points,
        clusters,
        silhouette: best.map(|km| km.silhouette),
        ..base
    }
}
