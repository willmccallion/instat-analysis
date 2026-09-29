//! Shots for and against where the match report's shooting charts drew them.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::common::PlayerRef;
use crate::model::{ChartedShot, Date, Game, GameId, Jersey, PlayerId, RinkPoint};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShotDot {
    pub game: GameId,
    pub date: Date,
    pub period: u32,
    pub shooter: Option<PlayerRef>,
    pub at: RinkPoint,
    pub goal: bool,
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
}

/// Every charted opponent shot in `games`, in game order.
pub(crate) fn shots_against(games: &[&Game]) -> Vec<ShotAgainstDot> {
    games
        .iter()
        .flat_map(|game| {
            game.charted_shots_against.iter().map(|shot| ShotAgainstDot {
                game: game.id.clone(),
                date: game.date,
                opponent: game.opponent.0.clone(),
                period: shot.period,
                jersey: shot.jersey,
                at: shot.at,
                goal: shot.goal,
            })
        })
        .collect()
}

/// Charted shots in `games` that `keep` accepts, in game order.
pub(crate) fn shot_dots(
    games: &[&Game],
    roster: &HashMap<PlayerId, PlayerRef>,
    keep: impl Fn(&ChartedShot) -> bool,
) -> Vec<ShotDot> {
    games
        .iter()
        .flat_map(|game| {
            game.charted_shots.iter().filter(|s| keep(s)).map(|shot| ShotDot {
                game: game.id.clone(),
                date: game.date,
                period: shot.period,
                shooter: shot.shooter.as_ref().and_then(|id| roster.get(id).cloned()),
                at: shot.at,
                goal: shot.goal,
            })
        })
        .collect()
}
