//! Stints: stretches of a game during which our on-ice skaters and the manpower state do
//! not change. Everything "who was on the ice together" is computed from these.

use std::collections::BTreeSet;

use crate::model::{Game, GameId, PlayerId, Seconds, Strength, Team};

/// Goals are attributed to the stint this long before the recorded goal time, so a line
/// change at the whistle does not move the goal to the next unit.
const GOAL_PROBE: f64 = 0.25;
const MIN_STINT_SECONDS: f64 = 0.05;

#[derive(Debug, Clone, PartialEq)]
pub struct Stint {
    pub game: GameId,
    pub start: Seconds,
    pub end: Seconds,
    /// Sorted.
    pub players: Vec<PlayerId>,
    pub strength: Strength,
    pub goals_for: u32,
    pub goals_against: u32,
}

impl Stint {
    #[must_use]
    pub fn duration(&self) -> Seconds {
        self.end - self.start
    }

    #[must_use]
    pub fn has(&self, id: &PlayerId) -> bool {
        self.players.binary_search(id).is_ok()
    }
}

/// Splits a game at every shift change, man-advantage boundary and period break.
#[must_use]
pub fn stints(game: &Game) -> Vec<Stint> {
    let mut cuts: BTreeSet<OrderedSeconds> = BTreeSet::new();
    cuts.insert(OrderedSeconds(0.0));
    cuts.insert(OrderedSeconds(game.length.0));
    for period in 1.. {
        let period_end = f64::from(period) * crate::model::PERIOD_SECONDS;
        if period_end >= game.length.0 {
            break;
        }
        cuts.insert(OrderedSeconds(period_end));
    }
    for player in &game.players {
        for shift in &player.shifts {
            cuts.insert(OrderedSeconds(shift.start.0));
            cuts.insert(OrderedSeconds(shift.end.0));
        }
    }
    for advantage in &game.advantages {
        cuts.insert(OrderedSeconds(advantage.interval.start.0));
        cuts.insert(OrderedSeconds(advantage.interval.end.0));
    }
    let boundaries: Vec<f64> = cuts.into_iter().map(|c| c.0).collect();
    let mut result: Vec<Stint> = boundaries
        .windows(2)
        .filter(|w| w[1] - w[0] > MIN_STINT_SECONDS)
        .map(|w| {
            let middle = Seconds(f64::midpoint(w[0], w[1]));
            let mut players: Vec<PlayerId> = game
                .players
                .iter()
                .filter(|p| p.shifts.iter().any(|s| s.start.0 <= middle.0 && middle.0 < s.end.0))
                .map(|p| p.id.clone())
                .collect();
            players.sort();
            Stint {
                game: game.id.clone(),
                start: Seconds(w[0]),
                end: Seconds(w[1]),
                players,
                strength: game.strength_at(middle),
                goals_for: 0,
                goals_against: 0,
            }
        })
        .collect();
    for goal in &game.goals {
        let probe = goal.time.0 - GOAL_PROBE;
        if let Some(stint) = result.iter_mut().find(|s| s.start.0 <= probe && probe < s.end.0) {
            match goal.scored_by {
                Team::Us => stint.goals_for += 1,
                Team::Them => stint.goals_against += 1,
            }
        }
    }
    result
}

/// `f64` with a total order, for use as a set key (game times are finite).
#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedSeconds(f64);

impl Eq for OrderedSeconds {}

impl PartialOrd for OrderedSeconds {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedSeconds {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}
