//! Our shots where the match report's shooting chart drew them.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::common::PlayerRef;
use crate::model::{ChartedShot, Date, Game, GameId, PlayerId, RinkPoint};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShotDot {
    pub game: GameId,
    pub date: Date,
    pub period: u32,
    pub shooter: Option<PlayerRef>,
    pub at: RinkPoint,
    pub goal: bool,
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
