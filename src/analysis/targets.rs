//! Coach-set targets for players (a stat and the level to reach), tracked game by game.

use serde::{Deserialize, Serialize};

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::analysis::players::SkaterTotals;
use crate::analysis::rankings::{RatingInput, Side};
use crate::analysis::rating_setup::RatingStat;
use crate::model::{Date, Game, GameId, PlayerId};
use crate::stats::describe::mean;

/// Games that count as "recent" when judging a target.
const RECENT_GAMES: usize = 3;

/// The level a coach wants a stat at; any finite number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct TargetLevel(f64);

impl TargetLevel {
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for TargetLevel {
    type Error = String;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() { Ok(Self(value)) } else { Err(format!("target {value} is not a number")) }
    }
}

impl From<TargetLevel> for f64 {
    fn from(level: TargetLevel) -> Self {
        level.0
    }
}

/// One target a coach set for one player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerTarget {
    pub player: PlayerId,
    pub stat: RatingStat,
    pub target: TargetLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TargetStatus {
    /// The last few games meet the target.
    Met,
    /// Not there yet, but the last few games are better than before them.
    Improving,
    NotYet,
    /// No games with a value yet.
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetGame {
    pub game: GameId,
    pub date: Date,
    pub value: Option<f64>,
    pub met: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetProgress {
    /// The target as the coach saved it, so the page can edit or remove it.
    pub saved: PlayerTarget,
    pub player: Option<PlayerRef>,
    pub stat_name: String,
    pub higher_is_better: bool,
    /// Over every game in scope pooled together.
    pub season: Option<f64>,
    /// The average of the last few games' values.
    pub recent: Option<f64>,
    pub games: Vec<TargetGame>,
    pub games_met: usize,
    pub status: TargetStatus,
}

fn meets(value: f64, target: f64, higher_is_better: bool) -> bool {
    if higher_is_better { value >= target } else { value <= target }
}

fn team_even_strength(game: &Game) -> (f64, f64) {
    (f64::from(game.summary.even_strength_shots.0), f64::from(game.opponent_summary.even_strength_shots.0))
}

/// The stat's value for `player` over `games` pooled together.
fn value_over(player: &PlayerRef, stat: RatingStat, games: &[&Game]) -> Option<f64> {
    let mut totals = SkaterTotals::default();
    let mut team = (0.0, 0.0);
    for game in games {
        let Some(stats) = game.player(&player.id).and_then(|p| p.skater.as_ref()) else {
            continue;
        };
        totals.add(stats);
        let (f, a) = team_even_strength(game);
        team = (team.0 + f, team.1 + a);
    }
    if totals.games == 0 {
        return None;
    }
    let input = RatingInput {
        player: player.clone(),
        side: Side::Ours,
        totals,
        team_even_strength: team,
        qualified: true,
    };
    stat.value(&input)
}

fn status(values: &[f64], target: f64, higher_is_better: bool) -> TargetStatus {
    if values.is_empty() {
        return TargetStatus::NoData;
    }
    let split = values.len().saturating_sub(RECENT_GAMES);
    let (earlier, recent) = values.split_at(split);
    let Some(recent_mean) = mean(recent) else {
        return TargetStatus::NoData;
    };
    if meets(recent_mean, target, higher_is_better) {
        return TargetStatus::Met;
    }
    match mean(earlier) {
        Some(before) if (recent_mean > before) == higher_is_better && (recent_mean - before).abs() > f64::EPSILON => TargetStatus::Improving,
        _ => TargetStatus::NotYet,
    }
}

fn progress(context: &Context<'_>, saved: &PlayerTarget) -> TargetProgress {
    let player = context.roster.get(&saved.player).cloned();
    let higher_is_better = saved.stat.higher_is_better();
    let target = saved.target.get();
    let games: Vec<TargetGame> = player
        .as_ref()
        .map(|p| {
            context
                .scope
                .iter()
                .filter(|g| g.player(&p.id).is_some_and(|x| x.skater.is_some()))
                .map(|g| {
                    let value = value_over(p, saved.stat, &[*g]);
                    TargetGame { game: g.id.clone(), date: g.date, value, met: value.map(|v| meets(v, target, higher_is_better)) }
                })
                .collect()
        })
        .unwrap_or_default();
    let values: Vec<f64> = games.iter().filter_map(|g| g.value).collect();
    let recent = mean(&values[values.len().saturating_sub(RECENT_GAMES)..]);
    TargetProgress {
        season: player.as_ref().and_then(|p| value_over(p, saved.stat, &context.scope)),
        recent,
        games_met: games.iter().filter(|g| g.met == Some(true)).count(),
        status: status(&values, target, higher_is_better),
        stat_name: saved.stat.name().to_owned(),
        higher_is_better,
        player,
        games,
        saved: saved.clone(),
    }
}

#[must_use]
pub fn targets(context: &Context<'_>, saved: &[PlayerTarget]) -> Vec<TargetProgress> {
    saved.iter().map(|t| progress(context, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_a_target_depends_on_which_way_is_better() {
        assert!(meets(12.0, 10.0, true));
        assert!(!meets(12.0, 10.0, false));
        assert!(meets(10.0, 10.0, false));
    }

    #[test]
    fn recent_games_decide_the_status() {
        assert_eq!(status(&[4.0, 5.0, 11.0, 12.0, 10.5], 10.0, true), TargetStatus::Met);
        assert_eq!(status(&[4.0, 5.0, 6.0, 8.0, 7.0], 10.0, true), TargetStatus::Improving);
        assert_eq!(status(&[9.0, 9.0, 6.0, 5.0, 7.0], 10.0, true), TargetStatus::NotYet);
        assert_eq!(status(&[3.0, 3.0, 2.0, 2.0, 2.0], 1.5, false), TargetStatus::Improving);
        assert_eq!(status(&[], 1.0, true), TargetStatus::NoData);
    }

    #[test]
    fn a_target_must_be_a_number() {
        assert!(TargetLevel::try_from(f64::NAN).is_err());
        assert!(serde_json::from_str::<TargetLevel>("12.5").is_ok());
    }
}
