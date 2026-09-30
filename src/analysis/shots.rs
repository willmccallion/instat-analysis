//! Shots for and against where the match report's shooting charts drew them.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::model::{ChartedShot, Date, Game, GameId, Jersey, RinkPoint};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShotDot {
    pub game: GameId,
    pub date: Date,
    pub period: u32,
    pub shooter: Option<PlayerRef>,
    pub at: RinkPoint,
    pub goal: bool,
    /// From [`xg`](super::xg); `None` when no model could be fitted.
    pub xg: Option<f64>,
}

/// An opponent's shot on our net; `at` is measured toward our goal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShotAgainstDot {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub period: u32,
    pub jersey: Option<Jersey>,
    pub at: RinkPoint,
    pub goal: bool,
    pub xg: Option<f64>,
}

/// Every charted opponent shot in `games`, in game order.
pub(crate) fn shots_against(context: &Context<'_>, games: &[&Game]) -> Vec<ShotAgainstDot> {
    games
        .iter()
        .flat_map(|game| {
            game.charted_shots_against.iter().enumerate().map(|(i, shot)| ShotAgainstDot {
                game: game.id.clone(),
                date: game.date,
                opponent: game.opponent.0.clone(),
                period: shot.period,
                jersey: shot.jersey,
                at: shot.at,
                goal: shot.goal,
                xg: context.shot_xg.theirs(&game.id, i),
            })
        })
        .collect()
}

/// Charted shots in `games` that `keep` accepts, in game order.
pub(crate) fn shot_dots(context: &Context<'_>, games: &[&Game], keep: impl Fn(&ChartedShot) -> bool) -> Vec<ShotDot> {
    games
        .iter()
        .flat_map(|game| {
            game.charted_shots
                .iter()
                .enumerate()
                .filter(|(_, s)| keep(s))
                .map(|(i, shot)| ShotDot {
                    game: game.id.clone(),
                    date: game.date,
                    period: shot.period,
                    shooter: shot.shooter.as_ref().and_then(|id| context.roster.get(id).cloned()),
                    at: shot.at,
                    goal: shot.goal,
                    xg: context.shot_xg.ours(&game.id, i),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
