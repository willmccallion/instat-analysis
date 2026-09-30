//! Unusual games and look-alikes: each game's team stats as standard scores against the
//! games in scope, the ones that stand out, and the past games it most resembles.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::share_pct;
use crate::analysis::team::{Outcome, outcome};
use crate::model::{Date, Game, GameId};
use crate::stats::describe::{mean, sample_sd};
use crate::stats::dist::chi_square_sf;

pub const MIN_GAMES: usize = 5;
/// A stat this many standard deviations from the usual is called out.
const STANDOUT_Z: f64 = 1.5;
const LOOK_ALIKES: usize = 3;

type Feature = (&'static str, fn(&Game) -> Option<f64>);

/// Our share of each stat (percent), so opponents of different strength compare on one scale.
const FEATURES: [Feature; 9] = [
    ("Shot attempt share", |g| share_pct(f64::from(g.summary.shots), f64::from(g.opponent_summary.shots))),
    ("Shots on goal share", |g| share_pct(f64::from(g.summary.shots_on_goal), f64::from(g.opponent_summary.shots_on_goal))),
    ("Expected-goals share", |g| g.summary.xg.zip(g.opponent_summary.xg).and_then(|(a, b)| share_pct(a, b))),
    ("Scoring-chance share", |g| share_pct(f64::from(g.summary.scoring_chances.0), f64::from(g.opponent_summary.scoring_chances.0))),
    ("Faceoff win %", |g| share_pct(f64::from(g.summary.faceoffs_won), f64::from(g.opponent_summary.faceoffs_won))),
    ("Puck battles won %", |g| share_pct(f64::from(g.summary.puck_battles_won), f64::from(g.opponent_summary.puck_battles_won))),
    ("Possession %", |g| g.summary.possession_pct),
    ("Hits share", |g| share_pct(f64::from(g.summary.hits), f64::from(g.opponent_summary.hits))),
    ("Power plays minus times short-handed", |g| Some(f64::from(g.summary.power_plays) - f64::from(g.opponent_summary.power_plays))),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Standout {
    pub stat: String,
    pub value: f64,
    pub usual: f64,
    pub z: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookAlike {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub goals_for: u32,
    pub goals_against: u32,
    pub outcome: Outcome,
    /// Root-mean-square gap in standard scores (0 = identical profile).
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameProfile {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub outcome: Outcome,
    /// Mean squared standard score over the stats: about 1 for a typical game.
    pub unusualness: f64,
    /// Chance a typical game looks at least this different, if the stats were independent.
    pub p: Option<f64>,
    pub standouts: Vec<Standout>,
    /// Earlier games only, closest first.
    pub look_alikes: Vec<LookAlike>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimilarityReport {
    pub ready: bool,
    pub needs: usize,
    pub stats: Vec<String>,
    pub games: Vec<GameProfile>,
}

/// Standard scores per game (rows) and stat (columns), `None` where a game lacks the stat,
/// with each stat's (mean, sd) over the games.
struct Scores {
    z: Vec<Vec<Option<f64>>>,
    spread: Vec<(f64, f64)>,
}

fn standard_scores(games: &[&Game]) -> Scores {
    let raw: Vec<Vec<Option<f64>>> = games.iter().map(|g| FEATURES.iter().map(|(_, f)| f(g)).collect()).collect();
    let spread: Vec<(f64, f64)> = (0..FEATURES.len())
        .map(|j| {
            let column: Vec<f64> = raw.iter().filter_map(|r| r[j]).collect();
            (mean(&column).unwrap_or(0.0), sample_sd(&column).unwrap_or(0.0))
        })
        .collect();
    let z = raw
        .iter()
        .map(|row| {
            row.iter()
                .zip(&spread)
                .map(|(v, (m, sd))| v.filter(|_| *sd > 0.0).map(|v| (v - m) / sd))
                .collect()
        })
        .collect();
    Scores { z, spread }
}

fn distance(a: &[Option<f64>], b: &[Option<f64>]) -> Option<f64> {
    let gaps: Vec<f64> = a.iter().zip(b).filter_map(|(x, y)| Some((x.as_ref()? - y.as_ref()?).powi(2))).collect();
    (!gaps.is_empty()).then(|| (gaps.iter().sum::<f64>() / gaps.len() as f64).sqrt())
}

#[must_use]
pub fn similarity(context: &Context<'_>) -> SimilarityReport {
    let games = &context.scope;
    let stats = FEATURES.iter().map(|(name, _)| (*name).to_owned()).collect();
    if games.len() < MIN_GAMES {
        return SimilarityReport { ready: false, needs: MIN_GAMES, stats, games: Vec::new() };
    }
    let Scores { z, spread } = standard_scores(games);
    let profiles = games
        .iter()
        .enumerate()
        .map(|(i, game)| {
            let present: Vec<f64> = z[i].iter().flatten().copied().collect();
            let chi: f64 = present.iter().map(|v| v * v).sum();
            let standouts = FEATURES
                .iter()
                .zip(&z[i])
                .zip(&spread)
                .filter_map(|(((name, f), zv), (m, _))| {
                    let zv = (*zv)?;
                    (zv.abs() >= STANDOUT_Z).then(|| Standout { stat: (*name).to_owned(), value: f(game).unwrap_or(*m), usual: *m, z: zv })
                })
                .collect();
            let mut look_alikes: Vec<LookAlike> = games
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && other.date < game.date)
                .filter_map(|(j, other)| {
                    Some(LookAlike {
                        game: other.id.clone(),
                        date: other.date,
                        opponent: other.opponent.0.clone(),
                        goals_for: other.goals_for,
                        goals_against: other.goals_against,
                        outcome: outcome(other),
                        distance: distance(&z[i], &z[j])?,
                    })
                })
                .collect();
            look_alikes.sort_by(|a, b| a.distance.total_cmp(&b.distance));
            look_alikes.truncate(LOOK_ALIKES);
            GameProfile {
                game: game.id.clone(),
                date: game.date,
                opponent: game.opponent.0.clone(),
                outcome: outcome(game),
                unusualness: if present.is_empty() { 0.0 } else { chi / present.len() as f64 },
                p: (!present.is_empty()).then(|| chi_square_sf(chi, present.len() as f64)).flatten(),
                standouts,
                look_alikes,
            }
        })
        .collect();
    SimilarityReport { ready: true, needs: MIN_GAMES, stats, games: profiles }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_profiles_are_zero_apart_and_missing_stats_are_skipped() {
        let a = [Some(1.0), Some(-0.5), None];
        let b = [Some(1.0), Some(-0.5), Some(2.0)];
        assert!(distance(&a, &b).unwrap().abs() < 1e-12);
        assert!((distance(&[Some(0.0), Some(0.0)], &[Some(2.0), Some(0.0)]).unwrap() - 2.0_f64.sqrt()).abs() < 1e-12);
        assert!(distance(&[None], &[Some(1.0)]).is_none());
    }
}
