//! A chance of scoring for every charted shot, ours and the opponent's.
//!
//! InStat prints xG per player and per team but not per shot. The shooting charts place every
//! shot, so a distance-and-angle logistic model is fitted so that each player's charted shots
//! add up to the xG InStat credited them with in that game (and the opponent's shots to their
//! team xG). Each shot's value is then scaled within its group so the totals match InStat
//! exactly: location decides how a group's xG is shared out, InStat decides the total.

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

use crate::analysis::common::PlayerRef;
use crate::model::{Date, Game, GameId, PlayerId, RinkPoint};
use crate::stats::aggregate::{TotalGroup, fit_logistic_to_totals, probabilities};

const GOAL_LINE: f64 = 89.0;
/// Shots charted on top of the crease would otherwise send ln(distance) to −∞.
const MIN_DISTANCE: f64 = 3.0;
const FEATURES: usize = 3;
const MIN_GAMES_FOR_HOLDOUT: usize = 2;

/// Feet from the middle of the goal line to `at`.
#[must_use]
pub fn distance(at: RinkPoint) -> f64 {
    (GOAL_LINE - at.along.0).hypot(at.across.0).max(MIN_DISTANCE)
}

/// Radians away from straight on: 0 in front of the net, π/2 level with the goal line.
#[must_use]
pub fn angle(at: RinkPoint) -> f64 {
    at.across.0.abs().atan2(GOAL_LINE - at.along.0)
}

fn features(at: RinkPoint) -> [f64; FEATURES] {
    [1.0, distance(at).ln(), angle(at)]
}

/// Fitted chance of scoring from a spot, before any group scaling.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotModel {
    coefficients: DVector<f64>,
}

impl ShotModel {
    #[must_use]
    pub fn chance(&self, at: RinkPoint) -> f64 {
        let x = DMatrix::from_row_slice(1, FEATURES, &features(at));
        probabilities(&x, &self.coefficients)[0]
    }
}

/// Which charted list a shot sits in, and its index there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ShotRef {
    Ours(usize),
    Theirs(usize),
}

/// Shots whose values should add up to one of InStat's printed xG totals.
struct Group {
    game: GameId,
    date: Date,
    source: GroupSource,
    shots: Vec<(ShotRef, RinkPoint)>,
    instat_xg: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupSource {
    OurShooter(PlayerId),
    Opponent(String),
}

fn game_groups(game: &Game) -> Vec<Group> {
    let mut groups: Vec<Group> = game
        .players
        .iter()
        .filter_map(|player| {
            let xg = player.skater.as_ref()?.xg?;
            let shots: Vec<(ShotRef, RinkPoint)> = game
                .charted_shots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.shooter.as_ref() == Some(&player.id))
                .map(|(i, s)| (ShotRef::Ours(i), s.at))
                .collect();
            (!shots.is_empty()).then(|| Group {
                game: game.id.clone(),
                date: game.date,
                source: GroupSource::OurShooter(player.id.clone()),
                shots,
                instat_xg: xg,
            })
        })
        .collect();
    if let Some(xg) = game.opponent_summary.xg.filter(|_| !game.charted_shots_against.is_empty()) {
        groups.push(Group {
            game: game.id.clone(),
            date: game.date,
            source: GroupSource::Opponent(game.opponent.0.clone()),
            shots: game
                .charted_shots_against
                .iter()
                .enumerate()
                .map(|(i, s)| (ShotRef::Theirs(i), s.at))
                .collect(),
            instat_xg: xg,
        });
    }
    groups
}

fn fit(groups: &[&Group]) -> Option<ShotModel> {
    let points: Vec<RinkPoint> = groups.iter().flat_map(|g| g.shots.iter().map(|(_, at)| *at)).collect();
    let x = DMatrix::from_fn(points.len(), FEATURES, |i, j| features(points[i])[j]);
    let mut next = 0;
    let totals: Vec<TotalGroup> = groups
        .iter()
        .map(|g| {
            let rows: Vec<usize> = (next..next + g.shots.len()).collect();
            next += g.shots.len();
            TotalGroup { rows, total: g.instat_xg }
        })
        .collect();
    fit_logistic_to_totals(&x, &totals).map(|f| ShotModel { coefficients: f.coefficients })
}

fn model_total(model: &ShotModel, group: &Group) -> f64 {
    group.shots.iter().map(|(_, at)| model.chance(*at)).sum()
}

/// xG for every charted shot of every game the model has seen.
#[derive(Debug, Clone, Default)]
pub struct ShotXg {
    by_game: HashMap<GameId, HashMap<ShotRef, f64>>,
}

impl ShotXg {
    /// Our `index`-th charted shot in `game`.
    #[must_use]
    pub fn ours(&self, game: &GameId, index: usize) -> Option<f64> {
        self.by_game.get(game)?.get(&ShotRef::Ours(index)).copied()
    }

    /// The opponent's `index`-th charted shot in `game`.
    #[must_use]
    pub fn theirs(&self, game: &GameId, index: usize) -> Option<f64> {
        self.by_game.get(game)?.get(&ShotRef::Theirs(index)).copied()
    }
}

/// Model values for every charted shot, scaled within each group to InStat's total; shots
/// outside any group (unknown shooter, no xG printed) keep the model's value.
fn shot_values(model: &ShotModel, games: &[&Game], groups: &[Group]) -> ShotXg {
    let mut by_game: HashMap<GameId, HashMap<ShotRef, f64>> = HashMap::new();
    for game in games {
        let values = by_game.entry(game.id.clone()).or_default();
        for (i, s) in game.charted_shots.iter().enumerate() {
            values.insert(ShotRef::Ours(i), model.chance(s.at));
        }
        for (i, s) in game.charted_shots_against.iter().enumerate() {
            values.insert(ShotRef::Theirs(i), model.chance(s.at));
        }
    }
    for group in groups {
        let modelled = model_total(model, group);
        if modelled <= 0.0 {
            continue;
        }
        let scale = group.instat_xg / modelled;
        if let Some(values) = by_game.get_mut(&group.game) {
            for (shot, _) in &group.shots {
                if let Some(v) = values.get_mut(shot) {
                    *v *= scale;
                }
            }
        }
    }
    ShotXg { by_game }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationPoint {
    pub date: Date,
    /// Our shooter, or `None` for the opponent's whole team.
    pub shooter: Option<PlayerRef>,
    pub opponent: Option<String>,
    pub shots: usize,
    pub instat_xg: f64,
    /// What location alone predicts, before scaling to InStat's total.
    pub model_xg: f64,
}

/// Leave-one-game-out check: each game's group totals predicted by a model fitted without it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Holdout {
    pub games: usize,
    pub model_error: f64,
    /// Same, when every shot is given the training games' average xG per shot.
    pub flat_error: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CurvePoint {
    pub distance: f64,
    /// xG straight in front of the net, and from 45° off to the side.
    pub straight: f64,
    pub wide: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XgReport {
    pub ready: bool,
    pub games: usize,
    pub shots: usize,
    pub calibration: Vec<CalibrationPoint>,
    /// Share of the spread in our shooters' InStat xG that location explains (the opponent's
    /// one team total per game would dwarf the spread and flatter the fit).
    pub r_squared: Option<f64>,
    pub holdout: Option<Holdout>,
    pub curve: Vec<CurvePoint>,
}

fn holdout(groups: &[Group]) -> Option<Holdout> {
    let mut games: Vec<&GameId> = groups.iter().map(|g| &g.game).collect();
    games.sort();
    games.dedup();
    if games.len() < MIN_GAMES_FOR_HOLDOUT {
        return None;
    }
    let (mut model_error, mut flat_error, mut count) = (0.0, 0.0, 0.0);
    for held_out in &games {
        let (test, train): (Vec<&Group>, Vec<&Group>) = groups.iter().partition(|g| &&g.game == held_out);
        let model = fit(&train)?;
        let train_shots: usize = train.iter().map(|g| g.shots.len()).sum();
        let per_shot = train.iter().map(|g| g.instat_xg).sum::<f64>() / train_shots as f64;
        for g in test {
            model_error += (model_total(&model, g) - g.instat_xg).abs();
            flat_error += (per_shot * g.shots.len() as f64 - g.instat_xg).abs();
            count += 1.0;
        }
    }
    Some(Holdout {
        games: games.len(),
        model_error: model_error / count,
        flat_error: flat_error / count,
    })
}

fn r_squared(points: &[CalibrationPoint]) -> Option<f64> {
    let points: Vec<&CalibrationPoint> = points.iter().filter(|p| p.shooter.is_some()).collect();
    let n = points.len() as f64;
    let mean = points.iter().map(|p| p.instat_xg).sum::<f64>() / n;
    let total: f64 = points.iter().map(|p| (p.instat_xg - mean).powi(2)).sum();
    let residual: f64 = points.iter().map(|p| (p.instat_xg - p.model_xg).powi(2)).sum();
    (total > 0.0).then(|| 1.0 - residual / total)
}

fn curve(model: &ShotModel) -> Vec<CurvePoint> {
    let at = |feet: f64, radians: f64| RinkPoint {
        along: crate::model::Feet(GOAL_LINE - feet * radians.cos()),
        across: crate::model::Feet(feet * radians.sin()),
    };
    (1..=16)
        .map(|step| {
            let feet = 5.0 * f64::from(step);
            CurvePoint {
                distance: feet,
                straight: model.chance(at(feet, 0.0)),
                wide: model.chance(at(feet, std::f64::consts::FRAC_PI_4)),
            }
        })
        .collect()
}

/// Fits the model on `games` (every loaded game, so a shot's value doesn't depend on the scope).
#[must_use]
pub(crate) fn shot_xg(games: &[&Game], roster: &HashMap<PlayerId, PlayerRef>) -> (ShotXg, XgReport) {
    let groups: Vec<Group> = games.iter().flat_map(|g| game_groups(g)).collect();
    let model = fit(&groups.iter().collect::<Vec<_>>());
    let games_used = {
        let mut ids: Vec<&GameId> = groups.iter().map(|g| &g.game).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    };
    let Some(model) = model else {
        return (
            ShotXg::default(),
            XgReport {
                ready: false,
                games: games_used,
                shots: 0,
                calibration: Vec::new(),
                r_squared: None,
                holdout: None,
                curve: Vec::new(),
            },
        );
    };
    let calibration: Vec<CalibrationPoint> = groups
        .iter()
        .map(|g| CalibrationPoint {
            date: g.date,
            shooter: match &g.source {
                GroupSource::OurShooter(id) => roster.get(id).cloned(),
                GroupSource::Opponent(_) => None,
            },
            opponent: match &g.source {
                GroupSource::OurShooter(_) => None,
                GroupSource::Opponent(name) => Some(name.clone()),
            },
            shots: g.shots.len(),
            instat_xg: g.instat_xg,
            model_xg: model_total(&model, g),
        })
        .collect();
    let report = XgReport {
        ready: true,
        games: games_used,
        shots: groups.iter().map(|g| g.shots.len()).sum(),
        r_squared: r_squared(&calibration),
        holdout: holdout(&groups),
        curve: curve(&model),
        calibration,
    };
    (shot_values(&model, games, &groups), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Feet;

    fn at(along: f64, across: f64) -> RinkPoint {
        RinkPoint {
            along: Feet(along),
            across: Feet(across),
        }
    }

    #[test]
    fn distance_and_angle_are_measured_from_the_middle_of_the_goal_line() {
        assert!((distance(at(79.0, 0.0)) - 10.0).abs() < 1e-12);
        assert!(angle(at(79.0, 0.0)).abs() < 1e-12);
        assert!((angle(at(79.0, 10.0)) - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
        assert!((angle(at(79.0, -10.0)) - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    }

    #[test]
    fn a_shot_on_the_crease_has_a_finite_distance() {
        assert!((distance(at(GOAL_LINE, 0.0)) - MIN_DISTANCE).abs() < 1e-12);
    }

    #[test]
    fn closer_and_more_central_shots_score_more_when_the_data_says_so() {
        let model = ShotModel {
            coefficients: DVector::from_row_slice(&[1.0, -1.2, -0.8]),
        };
        assert!(model.chance(at(80.0, 0.0)) > model.chance(at(50.0, 0.0)));
        assert!(model.chance(at(70.0, 0.0)) > model.chance(at(70.0, 19.0)));
    }
}
