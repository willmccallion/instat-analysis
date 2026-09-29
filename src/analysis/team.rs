//! Team-level results: record, shot and xG shares, special teams, periods, zones, game log.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{Estimate, share_pct};
use crate::analysis::players::add_zones;
use crate::model::{Date, Game, GameId, PERIOD_SECONDS, Seconds, Strength, Team, ZoneShots};
use crate::stats::describe::{mean, quantile};
use crate::stats::random;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Win,
    Loss,
    OvertimeLoss,
}

#[must_use]
pub fn outcome(game: &Game) -> Outcome {
    if game.goals_for > game.goals_against {
        Outcome::Win
    } else if game.length.0 > 3.0 * PERIOD_SECONDS + 1.0 {
        Outcome::OvertimeLoss
    } else {
        Outcome::Loss
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameLogRow {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub goals_for: u32,
    pub goals_against: u32,
    pub outcome: Outcome,
    pub shots_for: u32,
    pub shots_against: u32,
    pub shots_on_goal_for: u32,
    pub shots_on_goal_against: u32,
    pub xg_for: Option<f64>,
    pub xg_against: Option<f64>,
    pub even_strength_corsi_pct: Option<f64>,
    pub power_play: (u32, u32),
    pub penalty_kill: (u32, u32),
    pub penalty_minutes: f64,
    pub faceoff_pct: Option<f64>,
    pub possession_pct: Option<f64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeriodSplit {
    pub period: u32,
    pub shots_for: u32,
    pub shots_against: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub possession_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneFaceoffs {
    pub zone: String,
    pub won: u32,
    pub lost: u32,
    pub pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrengthGoals {
    pub strength: Strength,
    pub goals_for: u32,
    pub goals_against: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreState {
    pub state: String,
    pub time: Seconds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cumulative {
    pub date: Date,
    pub points: u32,
    pub goals_for: u32,
    pub goals_against: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TeamReport {
    pub games: usize,
    pub wins: u32,
    pub losses: u32,
    pub overtime_losses: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub shots_for: u32,
    pub shots_against: u32,
    pub shots_on_goal_for: u32,
    pub shots_on_goal_against: u32,
    pub xg_for: f64,
    pub xg_against: f64,
    pub shot_share: Option<f64>,
    pub xg_share: Option<f64>,
    pub even_strength_corsi_pct: Option<f64>,
    pub shooting_pct: Option<f64>,
    pub save_pct: Option<f64>,
    /// Shooting % + save %; far from 100 suggests luck that tends to even out.
    pub pdo: Option<f64>,
    pub goals_minus_xg: Option<f64>,
    pub power_play_chances: u32,
    pub power_play_goals: u32,
    pub power_play_pct: Option<f64>,
    pub times_short_handed: u32,
    pub power_play_goals_against: u32,
    pub penalty_kill_pct: Option<f64>,
    pub periods: Vec<PeriodSplit>,
    pub faceoffs: Vec<ZoneFaceoffs>,
    pub strength_goals: Vec<StrengthGoals>,
    pub score_states: Vec<ScoreState>,
    pub game_log: Vec<GameLogRow>,
    pub cumulative: Vec<Cumulative>,
    /// Mean goal differential per game with a bootstrap 95% interval (≥ 3 games).
    pub goal_differential: Option<Estimate>,
    /// Mean per-game shot share with a bootstrap 95% interval (≥ 3 games).
    pub shot_share_by_game: Option<Estimate>,
    /// Our shots by zone (sum over skaters).
    pub shot_zones: Vec<ZoneShots>,
}

fn game_log(game: &Game) -> GameLogRow {
    let s = &game.summary;
    let o = &game.opponent_summary;
    let faceoffs_total = s.faceoffs_won + o.faceoffs_won;
    GameLogRow {
        game: game.id.clone(),
        date: game.date,
        opponent: game.opponent.0.clone(),
        goals_for: game.goals_for,
        goals_against: game.goals_against,
        outcome: outcome(game),
        shots_for: s.shots,
        shots_against: o.shots,
        shots_on_goal_for: s.shots_on_goal,
        shots_on_goal_against: o.shots_on_goal,
        xg_for: s.xg,
        xg_against: o.xg,
        even_strength_corsi_pct: share_pct(f64::from(s.even_strength_shots.0), f64::from(o.even_strength_shots.0)),
        power_play: (s.power_play_goals, s.power_plays),
        penalty_kill: (o.power_plays.saturating_sub(o.power_play_goals), o.power_plays),
        penalty_minutes: s.penalty_time.minutes(),
        faceoff_pct: (faceoffs_total > 0).then(|| 100.0 * f64::from(s.faceoffs_won) / f64::from(faceoffs_total)),
        possession_pct: s.possession_pct,
        warnings: game.warnings.clone(),
    }
}

fn periods(games: &[&Game]) -> Vec<PeriodSplit> {
    let mut result: Vec<PeriodSplit> = Vec::new();
    for game in games {
        for (index, (&(sf, _, _), &(sa, _, _))) in game
            .summary
            .shots_by_period
            .iter()
            .zip(&game.opponent_summary.shots_by_period)
            .enumerate()
        {
            let period = u32::try_from(index + 1).unwrap_or(u32::MAX);
            if result.len() <= index {
                result.push(PeriodSplit {
                    period,
                    shots_for: 0,
                    shots_against: 0,
                    goals_for: 0,
                    goals_against: 0,
                    possession_pct: None,
                });
            }
            result[index].shots_for += sf;
            result[index].shots_against += sa;
        }
        for goal in &game.goals {
            let index = (goal.period() as usize).saturating_sub(1);
            if let Some(split) = result.get_mut(index) {
                match goal.scored_by {
                    Team::Us => split.goals_for += 1,
                    Team::Them => split.goals_against += 1,
                }
            }
        }
    }
    for (index, split) in result.iter_mut().enumerate() {
        let values: Vec<f64> = games
            .iter()
            .filter_map(|g| g.summary.possession_pct_by_period.get(index).copied())
            .collect();
        split.possession_pct = mean(&values);
    }
    result
}

fn faceoffs(games: &[&Game]) -> Vec<ZoneFaceoffs> {
    let zones = ["Defensive zone", "Neutral zone", "Offensive zone"];
    zones
        .iter()
        .enumerate()
        .map(|(z, name)| {
            let opposite = 2 - z;
            let won: u32 = games.iter().map(|g| g.summary.faceoffs_won_by_zone[z]).sum();
            let lost: u32 = games.iter().map(|g| g.opponent_summary.faceoffs_won_by_zone[opposite]).sum();
            ZoneFaceoffs {
                zone: (*name).to_owned(),
                won,
                lost,
                pct: share_pct(f64::from(won), f64::from(lost)),
            }
        })
        .collect()
}

fn strength_goals(games: &[&Game]) -> Vec<StrengthGoals> {
    [Strength::Even, Strength::PowerPlay, Strength::ShortHanded]
        .into_iter()
        .map(|strength| {
            let count = |team: Team| {
                let u = games
                    .iter()
                    .flat_map(|g| g.goals.iter())
                    .filter(|goal| {
                        goal.scored_by == team
                            && match team {
                                Team::Us => goal.strength == strength,
                                Team::Them => opposite(goal.strength) == strength,
                            }
                    })
                    .count();
                u32::try_from(u).unwrap_or(u32::MAX)
            };
            StrengthGoals {
                strength,
                goals_for: count(Team::Us),
                goals_against: count(Team::Them),
            }
        })
        .collect()
}

/// Strength from the opponent's point of view, so their power-play goals count as
/// "power play" for them.
const fn opposite(ours: Strength) -> Strength {
    match ours {
        Strength::Even => Strength::Even,
        Strength::PowerPlay => Strength::ShortHanded,
        Strength::ShortHanded => Strength::PowerPlay,
    }
}

fn score_states(games: &[&Game]) -> Vec<ScoreState> {
    let mut leading = 0.0;
    let mut tied = 0.0;
    let mut trailing = 0.0;
    for game in games {
        let mut time = 0.0;
        let mut diff: i64 = 0;
        for goal in &game.goals {
            let span = (goal.time.0 - time).max(0.0);
            match diff.signum() {
                1 => leading += span,
                -1 => trailing += span,
                _ => tied += span,
            }
            time = goal.time.0;
            diff = i64::from(goal.score_after.0) - i64::from(goal.score_after.1);
        }
        let span = (game.length.0 - time).max(0.0);
        match diff.signum() {
            1 => leading += span,
            -1 => trailing += span,
            _ => tied += span,
        }
    }
    [("Leading", leading), ("Tied", tied), ("Trailing", trailing)]
        .into_iter()
        .map(|(state, time)| ScoreState {
            state: state.to_owned(),
            time: Seconds(time),
        })
        .collect()
}

const BOOTSTRAP_REPLICATES: usize = 4000;
const MIN_GAMES_FOR_BOOTSTRAP: usize = 3;

/// Percentile bootstrap of a per-game mean, resampling games.
#[must_use]
pub fn bootstrap_mean(values: &[f64], seed: u64) -> Option<Estimate> {
    if values.len() < MIN_GAMES_FOR_BOOTSTRAP {
        return None;
    }
    let mut rng = random::seeded(seed);
    let n = values.len();
    let means: Vec<f64> = (0..BOOTSTRAP_REPLICATES)
        .map(|_| (0..n).map(|_| values[random::index(&mut rng, n)]).sum::<f64>() / n as f64)
        .collect();
    Some(Estimate {
        value: mean(values)?,
        low: quantile(&means, 0.025)?,
        high: quantile(&means, 0.975)?,
    })
}

fn cumulative(log: &[GameLogRow]) -> Vec<Cumulative> {
    let mut points = 0;
    let mut goals_for = 0;
    let mut goals_against = 0;
    log.iter()
        .map(|row| {
            points += match row.outcome {
                Outcome::Win => 2,
                Outcome::OvertimeLoss => 1,
                Outcome::Loss => 0,
            };
            goals_for += row.goals_for;
            goals_against += row.goals_against;
            Cumulative {
                date: row.date,
                points,
                goals_for,
                goals_against,
            }
        })
        .collect()
}

fn shot_zones(games: &[&Game]) -> Vec<ZoneShots> {
    let mut zones = Vec::new();
    for skater in games.iter().flat_map(|g| g.players.iter()).filter_map(|p| p.skater.as_ref()) {
        add_zones(&mut zones, &skater.shot_zones);
    }
    zones
}

#[must_use]
pub fn team(context: &Context<'_>) -> TeamReport {
    let games = &context.scope;
    let log: Vec<GameLogRow> = games.iter().map(|g| game_log(g)).collect();
    let sum = |f: fn(&GameLogRow) -> u32| log.iter().map(f).sum::<u32>();
    let count = |o: Outcome| u32::try_from(log.iter().filter(|r| r.outcome == o).count()).unwrap_or(u32::MAX);
    let (goals_for, goals_against) = (sum(|r| r.goals_for), sum(|r| r.goals_against));
    let (shots_for, shots_against) = (sum(|r| r.shots_for), sum(|r| r.shots_against));
    let (sog_for, sog_against) = (sum(|r| r.shots_on_goal_for), sum(|r| r.shots_on_goal_against));
    let xg_for: f64 = log.iter().filter_map(|r| r.xg_for).sum();
    let xg_against: f64 = log.iter().filter_map(|r| r.xg_against).sum();
    let ev_for: u32 = games.iter().map(|g| g.summary.even_strength_shots.0).sum();
    let ev_against: u32 = games.iter().map(|g| g.opponent_summary.even_strength_shots.0).sum();
    let f = f64::from;
    let shooting_pct = (sog_for > 0).then(|| 100.0 * f(goals_for) / f(sog_for));
    let save_pct = (sog_against > 0).then(|| 100.0 * (1.0 - f(goals_against) / f(sog_against)));
    let pp_chances = games.iter().map(|g| g.summary.power_plays).sum::<u32>();
    let pp_goals = games.iter().map(|g| g.summary.power_play_goals).sum::<u32>();
    let short_handed = games.iter().map(|g| g.opponent_summary.power_plays).sum::<u32>();
    let pp_against = games.iter().map(|g| g.opponent_summary.power_play_goals).sum::<u32>();
    let differentials: Vec<f64> = log.iter().map(|r| f(r.goals_for) - f(r.goals_against)).collect();
    let shot_shares: Vec<f64> = log
        .iter()
        .filter_map(|r| share_pct(f(r.shots_for), f(r.shots_against)))
        .collect();
    TeamReport {
        games: games.len(),
        wins: count(Outcome::Win),
        losses: count(Outcome::Loss),
        overtime_losses: count(Outcome::OvertimeLoss),
        goals_for,
        goals_against,
        shots_for,
        shots_against,
        shots_on_goal_for: sog_for,
        shots_on_goal_against: sog_against,
        xg_for,
        xg_against,
        shot_share: share_pct(f(shots_for), f(shots_against)),
        xg_share: share_pct(xg_for, xg_against),
        even_strength_corsi_pct: share_pct(f(ev_for), f(ev_against)),
        shooting_pct,
        save_pct,
        pdo: shooting_pct.zip(save_pct).map(|(a, b)| a + b),
        goals_minus_xg: (xg_for > 0.0).then(|| f(goals_for) - xg_for),
        power_play_chances: pp_chances,
        power_play_goals: pp_goals,
        power_play_pct: (pp_chances > 0).then(|| 100.0 * f(pp_goals) / f(pp_chances)),
        times_short_handed: short_handed,
        power_play_goals_against: pp_against,
        penalty_kill_pct: (short_handed > 0).then(|| 100.0 * (1.0 - f(pp_against) / f(short_handed))),
        periods: periods(games),
        faceoffs: faceoffs(games),
        strength_goals: strength_goals(games),
        score_states: score_states(games),
        cumulative: cumulative(&log),
        game_log: log,
        shot_zones: shot_zones(games),
        goal_differential: bootstrap_mean(&differentials, 11),
        shot_share_by_game: bootstrap_mean(&shot_shares, 12),
    }
}
