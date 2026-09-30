//! What the InStat Index rewards.
//!
//! A ridge regression of each player-game's index on that game's box-score numbers, over
//! every skater in the games (both teams). Its residuals show which of our players InStat
//! rates above or below what their numbers alone would give.

use std::collections::BTreeMap;

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::model::{PlayerId, Position, SkaterStats};
use crate::stats::describe::{mean, sample_sd};
use crate::stats::glm::fit_ridge;
use crate::stats::random;

const MIN_OBSERVATIONS: usize = 20;
const FOLDS: usize = 5;
const LAMBDA_GRID: [f64; 9] = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 1000.0];

type Feature = (&'static str, fn(&SkaterStats, Position) -> f64);

const FEATURES: [Feature; 19] = [
    ("Goals", |s, _| f64::from(s.goals)),
    ("Assists", |s, _| f64::from(s.assists)),
    ("Shots on goal", |s, _| f64::from(s.shots_on_goal)),
    ("Shots missed or blocked", |s, _| f64::from(s.shots.saturating_sub(s.shots_on_goal))),
    ("+/-", |s, _| s.plus_minus as f64),
    ("Minutes played", |s, _| s.toi.minutes()),
    ("Shot attempts for on ice", |s, _| f64::from(s.corsi_for)),
    ("Shot attempts against on ice", |s, _| f64::from(s.corsi_against)),
    ("Battles won", |s, _| f64::from(s.puck_battles_won)),
    ("Battles lost", |s, _| f64::from(s.puck_battles.saturating_sub(s.puck_battles_won))),
    ("Recoveries", |s, _| f64::from(s.puck_recoveries)),
    ("Puck losses", |s, _| f64::from(s.puck_losses)),
    ("Zone entries", |s, _| f64::from(s.entries)),
    ("Faceoffs won", |s, _| f64::from(s.faceoffs_won)),
    ("Faceoffs lost", |s, _| f64::from(s.faceoffs.saturating_sub(s.faceoffs_won))),
    ("Hits", |s, _| f64::from(s.hits)),
    ("Blocked shots", |s, _| f64::from(s.blocked_shots)),
    ("Penalty minutes", |s, _| s.penalty_minutes.minutes()),
    ("Plays defence", |_, p| f64::from(u8::from(p == Position::Defence))),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstatWeight {
    pub stat: String,
    /// Index points for one typical game-to-game step (1 SD) more of the stat.
    pub per_sd: f64,
    /// Index points for one more (goal, minute, battle, …).
    pub per_unit: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstatGap {
    pub player: PlayerRef,
    pub games: usize,
    pub actual: f64,
    /// What the player's numbers alone predict.
    pub predicted: f64,
    pub gap: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstatReport {
    pub ready: bool,
    pub needs: usize,
    pub observations: usize,
    pub lambda: Option<f64>,
    /// Share of the spread in the index the numbers explain, on player-games held out.
    pub cv_r_squared: Option<f64>,
    /// Biggest effect first.
    pub weights: Vec<InstatWeight>,
    /// Our players, most over-rated by InStat first.
    pub gaps: Vec<InstatGap>,
}

struct Observation {
    player: Option<PlayerId>,
    features: Vec<f64>,
    index: f64,
}

fn observations(context: &Context<'_>) -> Vec<Observation> {
    let row = |s: &SkaterStats, position: Position| FEATURES.iter().map(|(_, f)| f(s, position)).collect::<Vec<f64>>();
    let mut result = Vec::new();
    for game in &context.scope {
        for player in &game.players {
            let Some(stats) = player.skater.as_ref().filter(|s| s.toi.0 > 0.0) else {
                continue;
            };
            let position = context.roster.get(&player.id).map_or(player.position, |p| p.position);
            if let (Some(index), Position::Forward | Position::Defence) = (stats.instat_index, position) {
                result.push(Observation { player: Some(player.id.clone()), features: row(stats, position), index });
            }
        }
        for skater in &game.opponent_skaters {
            if let Some(index) = skater.stats.instat_index.filter(|_| skater.stats.toi.0 > 0.0) {
                result.push(Observation { player: None, features: row(&skater.stats, skater.position), index });
            }
        }
    }
    result
}

/// Column means and SDs; constant columns get SD 0 and are left out of the fit.
fn scaling(data: &[Observation]) -> Vec<(f64, f64)> {
    (0..FEATURES.len())
        .map(|j| {
            let column: Vec<f64> = data.iter().map(|o| o.features[j]).collect();
            (mean(&column).unwrap_or(0.0), sample_sd(&column).unwrap_or(0.0))
        })
        .collect()
}

fn design(data: &[&Observation], scale: &[(f64, f64)]) -> DMatrix<f64> {
    DMatrix::from_fn(data.len(), scale.len(), |i, j| {
        let (m, sd) = scale[j];
        if sd > 0.0 { (data[i].features[j] - m) / sd } else { 0.0 }
    })
}

fn predict(x: &DMatrix<f64>, intercept: f64, beta: &DVector<f64>) -> DVector<f64> {
    (x * beta).map(|v| v + intercept)
}

/// Held-out R² for each λ over the same shuffled folds; the best one wins.
fn cross_validate(data: &[Observation], scale: &[(f64, f64)]) -> Option<(f64, f64)> {
    let mut order: Vec<usize> = (0..data.len()).collect();
    random::shuffle(&mut random::seeded(23), &mut order);
    let fold_of: Vec<usize> = {
        let mut f = vec![0; data.len()];
        for (rank, &i) in order.iter().enumerate() {
            f[i] = rank % FOLDS;
        }
        f
    };
    let y_mean = data.iter().map(|o| o.index).sum::<f64>() / data.len() as f64;
    let total: f64 = data.iter().map(|o| (o.index - y_mean).powi(2)).sum();
    if total <= 0.0 {
        return None;
    }
    LAMBDA_GRID
        .iter()
        .filter_map(|&lambda| {
            let mut residual = 0.0;
            for fold in 0..FOLDS {
                let train: Vec<&Observation> = data.iter().zip(&fold_of).filter(|(_, f)| **f != fold).map(|(o, _)| o).collect();
                let test: Vec<&Observation> = data.iter().zip(&fold_of).filter(|(_, f)| **f == fold).map(|(o, _)| o).collect();
                let y = DVector::from_iterator(train.len(), train.iter().map(|o| o.index));
                let (intercept, beta) = fit_ridge(&design(&train, scale), &y, lambda)?;
                let fitted = predict(&design(&test, scale), intercept, &beta);
                residual += test.iter().zip(fitted.iter()).map(|(o, f)| (o.index - f).powi(2)).sum::<f64>();
            }
            Some((lambda, 1.0 - residual / total))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn gaps(context: &Context<'_>, data: &[Observation], fitted: &DVector<f64>) -> Vec<InstatGap> {
    let mut by_player: BTreeMap<&PlayerId, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for (o, f) in data.iter().zip(fitted.iter()) {
        if let Some(id) = &o.player {
            let entry = by_player.entry(id).or_default();
            entry.0.push(o.index);
            entry.1.push(*f);
        }
    }
    let mut rows: Vec<InstatGap> = by_player
        .into_iter()
        .filter_map(|(id, (actual, predicted))| {
            let (actual, predicted_mean) = (mean(&actual)?, mean(&predicted)?);
            Some(InstatGap {
                player: context.roster.get(id)?.clone(),
                games: predicted.len(),
                actual,
                predicted: predicted_mean,
                gap: actual - predicted_mean,
            })
        })
        .collect();
    rows.sort_by(|a, b| b.gap.total_cmp(&a.gap));
    rows
}

#[must_use]
pub fn instat(context: &Context<'_>) -> InstatReport {
    let data = observations(context);
    let not_ready = InstatReport {
        ready: false,
        needs: MIN_OBSERVATIONS,
        observations: data.len(),
        lambda: None,
        cv_r_squared: None,
        weights: Vec::new(),
        gaps: Vec::new(),
    };
    if data.len() < MIN_OBSERVATIONS {
        return not_ready;
    }
    let scale = scaling(&data);
    let Some((lambda, cv_r_squared)) = cross_validate(&data, &scale) else {
        return not_ready;
    };
    let all: Vec<&Observation> = data.iter().collect();
    let x = design(&all, &scale);
    let y = DVector::from_iterator(data.len(), data.iter().map(|o| o.index));
    let Some((intercept, beta)) = fit_ridge(&x, &y, lambda) else {
        return not_ready;
    };
    let mut weights: Vec<InstatWeight> = FEATURES
        .iter()
        .zip(&scale)
        .zip(beta.iter())
        .filter(|((_, (_, sd)), _)| *sd > 0.0)
        .map(|(((name, _), (_, sd)), b)| InstatWeight { stat: (*name).to_owned(), per_sd: *b, per_unit: b / sd })
        .collect();
    weights.sort_by(|a, b| b.per_sd.abs().total_cmp(&a.per_sd.abs()));
    let fitted = predict(&x, intercept, &beta);
    InstatReport {
        ready: true,
        needs: MIN_OBSERVATIONS,
        observations: data.len(),
        lambda: Some(lambda),
        cv_r_squared: Some(cv_r_squared),
        weights,
        gaps: gaps(context, &data, &fitted),
    }
}
