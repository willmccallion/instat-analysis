//! Individual impact separated from linemates (RAPM-style ridge Poisson regressions), and
//! the additive-model expectation used to judge pair chemistry.

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::analysis::stints::Stint;
use crate::model::{GameId, PlayerId, Seconds, Strength, UnitKind, UnitStats};
use crate::stats::glm::{PoissonFit, PoissonProblem, fit_poisson};

/// Ridge strengths tried by leave-one-game-out cross-validation.
const LAMBDA_GRID: [f64; 8] = [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0];
const DEFAULT_CORSI_LAMBDA: f64 = 5.0;
const DEFAULT_GOAL_LAMBDA: f64 = 20.0;
const MIN_GAMES_FOR_CV: usize = 3;
/// Tiny ridge used for likelihood-ratio tests so that aliased columns stay estimable.
const TEST_RIDGE: f64 = 1e-4;

/// One observation: a group of players on the ice for `toi`, producing `count` events.
#[derive(Debug, Clone)]
pub struct Observation {
    pub game: GameId,
    pub players: Vec<PlayerId>,
    pub toi: Seconds,
    pub count_for: f64,
    pub count_against: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpactRow {
    pub player: PlayerRef,
    pub toi: Seconds,
    /// Change in events for per 60 minutes versus an average teammate.
    pub for_per_60: f64,
    /// Change in events against per 60 (negative is good).
    pub against_per_60: f64,
    pub net_per_60: f64,
    pub for_se: f64,
    pub against_se: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpactModel {
    pub label: String,
    pub lambda: f64,
    pub lambda_from_cv: bool,
    pub observations: usize,
    pub baseline_for_60: f64,
    pub baseline_against_60: f64,
    pub rows: Vec<ImpactRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpactReport {
    pub corsi_defence: Option<ImpactModel>,
    pub corsi_forwards: Option<ImpactModel>,
    pub goals: Option<ImpactModel>,
}

/// Fitted additive ratings that predict a pair's expected shares.
#[derive(Debug, Clone, Default)]
pub struct Ratings {
    pub intercept_for: f64,
    pub intercept_against: f64,
    pub offence: HashMap<PlayerId, f64>,
    pub defence: HashMap<PlayerId, f64>,
}

/// Events for and against per 60 minutes that the additive model expects of a group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpectedRates {
    pub for_per_60: f64,
    pub against_per_60: f64,
}

impl Ratings {
    /// Rates for a group with every other player on the ice average.
    #[must_use]
    pub fn expected_rates(&self, players: &[&PlayerId]) -> Option<ExpectedRates> {
        let mut log_for = self.intercept_for;
        let mut log_against = self.intercept_against;
        for id in players {
            log_for += self.offence.get(*id)?;
            log_against += self.defence.get(*id)?;
        }
        Some(ExpectedRates { for_per_60: log_for.exp(), against_per_60: log_against.exp() })
    }

    /// Expected CF% for a group under the additive model.
    #[must_use]
    pub fn expected_share(&self, players: &[&PlayerId]) -> Option<f64> {
        let rates = self.expected_rates(players)?;
        Some(100.0 * rates.for_per_60 / (rates.for_per_60 + rates.against_per_60))
    }
}

pub struct Design {
    pub players: Vec<PlayerId>,
    pub x: DMatrix<f64>,
    pub offset: DVector<f64>,
}

#[must_use]
pub fn design(observations: &[Observation], extra_columns: &[Vec<f64>]) -> Design {
    let mut players: Vec<PlayerId> = observations.iter().flat_map(|o| o.players.iter().cloned()).collect();
    players.sort();
    players.dedup();
    let index: HashMap<&PlayerId, usize> = players.iter().enumerate().map(|(i, p)| (p, i)).collect();
    let n = observations.len();
    let p = 1 + players.len() + extra_columns.len();
    let mut x = DMatrix::zeros(n, p);
    for (row, observation) in observations.iter().enumerate() {
        x[(row, 0)] = 1.0;
        for id in &observation.players {
            if let Some(&column) = index.get(id) {
                x[(row, 1 + column)] = 1.0;
            }
        }
        for (k, column) in extra_columns.iter().enumerate() {
            x[(row, 1 + players.len() + k)] = column[row];
        }
    }
    let offset = DVector::from_iterator(n, observations.iter().map(|o| (o.toi.0 / 3600.0).max(1e-6).ln()));
    Design { players, x, offset }
}

fn penalty(p: usize, lambda: f64) -> DVector<f64> {
    DVector::from_fn(p, |j, _| if j == 0 { 0.0 } else { lambda })
}

#[must_use]
pub fn fit_side(d: &Design, y: &DVector<f64>, lambda: f64) -> Option<PoissonFit> {
    fit_poisson(&PoissonProblem {
        x: d.x.clone(),
        y: y.clone(),
        offset: d.offset.clone(),
        penalty: penalty(d.x.ncols(), lambda),
    })
}

fn rows_subset(d: &Design, y: &DVector<f64>, keep: &[usize]) -> (Design, DVector<f64>) {
    let x = DMatrix::from_fn(keep.len(), d.x.ncols(), |i, j| d.x[(keep[i], j)]);
    let offset = DVector::from_fn(keep.len(), |i, _| d.offset[keep[i]]);
    let y = DVector::from_fn(keep.len(), |i, _| y[keep[i]]);
    (
        Design {
            players: d.players.clone(),
            x,
            offset,
        },
        y,
    )
}

/// Leave-one-game-out predictive deviance for each λ; returns the best.
fn choose_lambda(observations: &[Observation], d: &Design, y_for: &DVector<f64>, y_against: &DVector<f64>) -> Option<f64> {
    let mut games: Vec<&GameId> = observations.iter().map(|o| &o.game).collect();
    games.sort();
    games.dedup();
    if games.len() < MIN_GAMES_FOR_CV {
        return None;
    }
    let score = |lambda: f64| -> Option<f64> {
        let mut total = 0.0;
        for held_out in &games {
            let train: Vec<usize> = (0..observations.len()).filter(|&i| &&observations[i].game != held_out).collect();
            let test: Vec<usize> = (0..observations.len()).filter(|&i| &&observations[i].game == held_out).collect();
            for y in [y_for, y_against] {
                let (train_design, train_y) = rows_subset(d, y, &train);
                let fit = fit_side(&train_design, &train_y, lambda)?;
                let (test_design, test_y) = rows_subset(d, y, &test);
                let mu = (&test_design.x * &fit.coefficients + &test_design.offset).map(f64::exp);
                total += crate::stats::glm::poisson_deviance(&test_y, &mu);
            }
        }
        Some(total)
    };
    LAMBDA_GRID
        .iter()
        .filter_map(|&l| score(l).map(|s| (l, s)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(l, _)| l)
}

/// Fits for/against models and converts coefficients to per-60 effects.
#[must_use]
pub fn impact_model(
    context: &Context<'_>,
    label: &str,
    observations: &[Observation],
    default_lambda: f64,
) -> Option<(ImpactModel, Ratings)> {
    if observations.is_empty() {
        return None;
    }
    let d = design(observations, &[]);
    let y_for = DVector::from_iterator(observations.len(), observations.iter().map(|o| o.count_for));
    let y_against = DVector::from_iterator(observations.len(), observations.iter().map(|o| o.count_against));
    let cv = choose_lambda(observations, &d, &y_for, &y_against);
    let lambda = cv.unwrap_or(default_lambda);
    let fit_for = fit_side(&d, &y_for, lambda)?;
    let fit_against = fit_side(&d, &y_against, lambda)?;
    let base_for = fit_for.coefficients[0].exp();
    let base_against = fit_against.coefficients[0].exp();
    let mut toi: HashMap<&PlayerId, f64> = HashMap::new();
    for o in observations {
        for id in &o.players {
            *toi.entry(id).or_default() += o.toi.0;
        }
    }
    let mut ratings = Ratings {
        intercept_for: fit_for.coefficients[0],
        intercept_against: fit_against.coefficients[0],
        ..Ratings::default()
    };
    let mut rows = Vec::new();
    for (k, id) in d.players.iter().enumerate() {
        let (o, a) = (fit_for.coefficients[k + 1], fit_against.coefficients[k + 1]);
        ratings.offence.insert(id.clone(), o);
        ratings.defence.insert(id.clone(), a);
        let Some(player) = context.roster.get(id) else {
            continue;
        };
        let for_per_60 = base_for * o.exp_m1();
        let against_per_60 = base_against * a.exp_m1();
        rows.push(ImpactRow {
            player: player.clone(),
            toi: Seconds(toi.get(id).copied().unwrap_or_default()),
            for_per_60,
            against_per_60,
            net_per_60: for_per_60 - against_per_60,
            for_se: base_for * o.exp() * fit_for.standard_errors[k + 1],
            against_se: base_against * a.exp() * fit_against.standard_errors[k + 1],
        });
    }
    rows.sort_by(|x, y| y.net_per_60.total_cmp(&x.net_per_60));
    Some((
        ImpactModel {
            label: label.to_owned(),
            lambda,
            lambda_from_cv: cv.is_some(),
            observations: observations.len(),
            baseline_for_60: base_for,
            baseline_against_60: base_against,
            rows,
        },
        ratings,
    ))
}

#[must_use]
pub fn unit_observations(context: &Context<'_>, kind: UnitKind) -> Vec<Observation> {
    context
        .scope
        .iter()
        .flat_map(|game| {
            game.units.iter().filter(move |u| u.kind == kind).filter_map(move |u| match &u.stats {
                UnitStats::EvenStrength(s) if u.toi.0 > 0.0 => Some(Observation {
                    game: game.id.clone(),
                    players: u.players.clone(),
                    toi: u.toi,
                    count_for: f64::from(s.corsi_for),
                    count_against: f64::from(s.corsi_against),
                }),
                _ => None,
            })
        })
        .collect()
}

#[must_use]
pub fn stint_observations(stints: &[Stint]) -> Vec<Observation> {
    stints
        .iter()
        .filter(|s| s.strength == Strength::Even && !s.players.is_empty())
        .map(|s| Observation {
            game: s.game.clone(),
            players: s.players.clone(),
            toi: s.duration(),
            count_for: f64::from(s.goals_for),
            count_against: f64::from(s.goals_against),
        })
        .collect()
}

pub struct ImpactOutputs {
    pub report: ImpactReport,
    pub defence_ratings: Option<Ratings>,
    pub forward_ratings: Option<Ratings>,
    pub full_unit_ratings: Option<Ratings>,
    pub goal_ratings: Option<Ratings>,
}

#[must_use]
pub fn impact(context: &Context<'_>) -> ImpactOutputs {
    let defence = impact_model(
        context,
        "Shot attempts, defence (from defence-pair tables)",
        &unit_observations(context, UnitKind::DefencePair),
        DEFAULT_CORSI_LAMBDA,
    );
    let forwards = impact_model(
        context,
        "Shot attempts, forwards (from forward-line tables)",
        &unit_observations(context, UnitKind::ForwardLine),
        DEFAULT_CORSI_LAMBDA,
    );
    let full = impact_model(
        context,
        "Shot attempts, five-man units",
        &unit_observations(context, UnitKind::FullUnit),
        DEFAULT_CORSI_LAMBDA,
    );
    let goals = impact_model(
        context,
        "Even-strength goals (from shifts)",
        &stint_observations(&context.stints),
        DEFAULT_GOAL_LAMBDA,
    );
    ImpactOutputs {
        report: ImpactReport {
            corsi_defence: defence.as_ref().map(|(m, _)| m.clone()),
            corsi_forwards: forwards.as_ref().map(|(m, _)| m.clone()),
            goals: goals.as_ref().map(|(m, _)| m.clone()),
        },
        defence_ratings: defence.map(|(_, r)| r),
        forward_ratings: forwards.map(|(_, r)| r),
        full_unit_ratings: full.map(|(_, r)| r),
        goal_ratings: goals.map(|(_, r)| r),
    }
}

/// Deviance of additive-only and additive-plus-group-interaction fits (nearly unpenalised),
/// for the "is chemistry real?" likelihood-ratio test.
pub struct InteractionTest {
    pub delta_deviance: f64,
    pub df: f64,
    pub residual_deviance: f64,
    pub residual_df: f64,
    pub observations: usize,
    pub groups: usize,
}

pub(crate) fn rank(x: &DMatrix<f64>) -> usize {
    let svd = x.clone().svd(false, false);
    let largest = svd.singular_values.iter().copied().fold(0.0_f64, f64::max);
    svd.singular_values
        .iter()
        .filter(|s| **s > largest * 1e-9)
        .count()
}

#[must_use]
pub fn interaction_test(observations: &[Observation]) -> Option<InteractionTest> {
    let mut groups: Vec<Vec<PlayerId>> = observations
        .iter()
        .map(|o| {
            let mut p = o.players.clone();
            p.sort();
            p
        })
        .collect();
    groups.sort();
    groups.dedup();
    let columns: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            observations
                .iter()
                .map(|o| {
                    let mut p = o.players.clone();
                    p.sort();
                    if &p == g { 1.0 } else { 0.0 }
                })
                .collect()
        })
        .collect();
    let additive = design(observations, &[]);
    let full = design(observations, &columns);
    let df = rank(&full.x).checked_sub(rank(&additive.x))? as f64;
    let residual_df = observations.len() as f64 * 2.0 - 2.0 * rank(&full.x) as f64;
    if df <= 0.0 {
        return None;
    }
    let mut delta = 0.0;
    let mut residual = 0.0;
    for y in [
        DVector::from_iterator(observations.len(), observations.iter().map(|o| o.count_for)),
        DVector::from_iterator(observations.len(), observations.iter().map(|o| o.count_against)),
    ] {
        let small = fit_side(&additive, &y, TEST_RIDGE)?;
        let large = fit_side(&full, &y, TEST_RIDGE)?;
        delta += (small.deviance - large.deviance).max(0.0);
        residual += large.deviance;
    }
    Some(InteractionTest {
        delta_deviance: delta,
        df: 2.0 * df,
        residual_deviance: residual,
        residual_df,
        observations: observations.len(),
        groups: groups.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> PlayerId {
        PlayerId(s.to_owned())
    }

    /// Four defencemen rotate through all pairings over many games; D1 drives play.
    fn planted_observations(chemistry: bool, rng: &mut crate::stats::random::Rng) -> Vec<Observation> {
        let pairs = [("D1", "D2"), ("D1", "D3"), ("D1", "D4"), ("D2", "D3"), ("D2", "D4"), ("D3", "D4")];
        let strength: HashMap<&str, f64> = [("D1", 0.5), ("D2", 0.0), ("D3", 0.0), ("D4", -0.2)].into_iter().collect();
        let mut observations = Vec::new();
        for game in 0..12 {
            for (a, b) in pairs {
                let bonus = if chemistry && (a, b) == ("D2", "D3") { 0.6 } else { 0.0 };
                let minutes = 6.0;
                let rate_for = 50.0 * (strength[a] + strength[b] + bonus).exp();
                let rate_against = 50.0 * (-strength[a] - strength[b] - bonus).exp();
                observations.push(Observation {
                    game: GameId(format!("g{game}")),
                    players: vec![id(a), id(b)],
                    toi: Seconds(minutes * 60.0),
                    count_for: crate::stats::random::poisson(rng, rate_for * minutes / 60.0) as f64,
                    count_against: crate::stats::random::poisson(rng, rate_against * minutes / 60.0) as f64,
                });
            }
        }
        observations
    }

    #[test]
    fn strongest_player_gets_the_highest_offence_rating() {
        let mut rng = crate::stats::random::seeded(5);
        let observations = planted_observations(false, &mut rng);
        let d = design(&observations, &[]);
        let y = DVector::from_iterator(observations.len(), observations.iter().map(|o| o.count_for));
        let fit = fit_side(&d, &y, 1.0).unwrap();
        let d1 = d.players.iter().position(|p| p.0 == "D1").unwrap();
        let best = (0..d.players.len()).max_by(|&a, &b| fit.coefficients[a + 1].total_cmp(&fit.coefficients[b + 1])).unwrap();
        assert_eq!(best, d1);
    }

    #[test]
    fn interaction_test_has_nominal_false_positive_rate_and_high_power() {
        let mut rng = crate::stats::random::seeded(17);
        let p = |t: &InteractionTest| crate::stats::dist::chi_square_sf(t.delta_deviance, t.df).unwrap();
        let simulations = 200;
        let mut false_positives = 0;
        let mut detections = 0;
        for _ in 0..simulations {
            if p(&interaction_test(&planted_observations(false, &mut rng)).unwrap()) < 0.05 {
                false_positives += 1;
            }
            if p(&interaction_test(&planted_observations(true, &mut rng)).unwrap()) < 0.05 {
                detections += 1;
            }
        }
        let false_positive_rate = f64::from(false_positives) / f64::from(simulations);
        let power = f64::from(detections) / f64::from(simulations);
        assert!(false_positive_rate <= 0.09, "false positive rate {false_positive_rate}");
        assert!(power >= 0.9, "power {power}");
    }
}
