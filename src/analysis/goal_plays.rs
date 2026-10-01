//! How goals for and against came about over the games in scope: off a faceoff, a turnover
//! won in the zone, a rush or sustained pressure, and how the plays were built.

use serde::Serialize;

use crate::analysis::Context;
use crate::model::{Game, GoalOrigin, GoalPlay, PlayKind, Strength, Team};
use crate::stats::describe::mean;

/// Passes this long before a goal count towards its build-up.
const PASSES_WINDOW: f64 = 20.0;
/// A goal and its play are the same goal when their times are this close.
const SAME_GOAL: f64 = 0.5;

/// A count for our goals and for theirs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ForAgainst {
    pub ours: u32,
    pub theirs: u32,
}

impl ForAgainst {
    const fn add(&mut self, team: Team) {
        match team {
            Team::Us => self.ours += 1,
            Team::Them => self.theirs += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OriginRow {
    pub origin: GoalOrigin,
    pub goals: ForAgainst,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalPlaysReport {
    pub goals: ForAgainst,
    pub origins: Vec<OriginRow>,
    pub rebounds: ForAgainst,
    /// Scored on a power play.
    pub power_play: ForAgainst,
    /// Mean seconds from the faceoff (or period start) to the goal, (ours, theirs).
    pub build_up: (Option<f64>, Option<f64>),
    /// Mean completed passes by the scoring team in the last 20 seconds, (ours, theirs).
    pub passes: (Option<f64>, Option<f64>),
}

const ORIGINS: [GoalOrigin; 4] = [GoalOrigin::Faceoff, GoalOrigin::Turnover, GoalOrigin::Rush, GoalOrigin::Sustained];

fn on_power_play(game: &Game, play: &GoalPlay) -> bool {
    game.goals
        .iter()
        .find(|g| (g.time.0 - play.time.0).abs() < SAME_GOAL)
        .is_some_and(|g| matches!((play.scored_by, g.strength), (Team::Us, Strength::PowerPlay) | (Team::Them, Strength::ShortHanded)))
}

fn passes_before(play: &GoalPlay) -> f64 {
    let count = play
        .events
        .iter()
        .filter(|e| e.team == play.scored_by && e.kind == PlayKind::Pass && play.time.0 - e.time.0 <= PASSES_WINDOW)
        .count();
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

#[must_use]
pub fn goal_plays(context: &Context<'_>) -> GoalPlaysReport {
    let mut goals = ForAgainst::default();
    let mut rebounds = ForAgainst::default();
    let mut power_play = ForAgainst::default();
    let mut origins: Vec<OriginRow> = ORIGINS.iter().map(|&origin| OriginRow { origin, goals: ForAgainst::default() }).collect();
    let (mut build_up, mut passes) = ((Vec::new(), Vec::new()), (Vec::new(), Vec::new()));
    for game in &context.scope {
        for play in &game.goal_plays {
            goals.add(play.scored_by);
            if let Some(row) = origins.iter_mut().find(|r| r.origin == play.origin) {
                row.goals.add(play.scored_by);
            }
            if play.rebound {
                rebounds.add(play.scored_by);
            }
            if on_power_play(game, play) {
                power_play.add(play.scored_by);
            }
            let (seconds, count) = match play.scored_by {
                Team::Us => (&mut build_up.0, &mut passes.0),
                Team::Them => (&mut build_up.1, &mut passes.1),
            };
            seconds.push(play.time.0 - play.start.0);
            count.push(passes_before(play));
        }
    }
    GoalPlaysReport {
        goals,
        origins,
        rebounds,
        power_play,
        build_up: (mean(&build_up.0), mean(&build_up.1)),
        passes: (mean(&passes.0), mean(&passes.1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlayEvent, Seconds};

    fn pass(time: f64, team: Team) -> PlayEvent {
        PlayEvent { time: Seconds(time), team, player: None, name: None, kind: PlayKind::Pass, detail: "Pass".to_owned(), at: None }
    }

    #[test]
    fn only_the_scoring_teams_recent_passes_count() {
        let play = GoalPlay {
            time: Seconds(100.0),
            scored_by: Team::Us,
            origin: GoalOrigin::Sustained,
            rebound: false,
            start: Seconds(40.0),
            events: vec![pass(50.0, Team::Us), pass(85.0, Team::Us), pass(90.0, Team::Them), pass(95.0, Team::Us)],
        };

        assert!((passes_before(&play) - 2.0).abs() < f64::EPSILON);
    }
}
