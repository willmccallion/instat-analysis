//! Goaltender save percentages with intervals, splits, trends, and goals saved against what
//! the shots they faced were worth.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{Estimate, PlayerRef, Shrunk, fit_beta_prior, per_60, shrink};
use crate::analysis::luck::goals_from_shots;
use crate::model::{
    BodyArea, Date, Game, GoalieStats, HistoryKind, NetArea, PlayerId, ReboundControl, SaveSplits, Saves, Seconds,
    ShotDistance, StatEntry,
};
use crate::stats::describe::{mean, wilson_interval};

/// One period's shots against a goalie, from the opponent's shooting chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PeriodSaves {
    pub period: u32,
    pub attempts: u32,
    pub goals: u32,
    pub xg_against: f64,
    pub saved_above_expected: f64,
}

/// Goals the goalie kept out beyond what the attempts they faced were worth. xG covers every
/// attempt, including ones that missed or were blocked, so this also reflects the defence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalsSaved {
    pub xg_against: f64,
    pub goals_against: u32,
    pub saved_above_expected: f64,
    pub per_60: Option<f64>,
    /// Chance an average goalie facing the same charted shots allows no more goals than this
    /// one did (small = clearly better than average); only when every game was theirs alone
    /// and charted.
    pub chance_average_does_as_well: Option<f64>,
    /// Games shared with another goalie: their xG against is the team's, split by shots faced.
    pub shared_games: u32,
    /// Games where this goalie was alone in net, by period.
    pub by_period: Vec<PeriodSaves>,
}

/// Save % in one category, pulled toward the goalie's own average in proportion to how
/// few shots it has.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ShrunkSaves<K> {
    pub kind: K,
    pub shots: u32,
    pub saves: u32,
    pub save_pct: Shrunk,
    /// Posterior probability the true save % here is below the goalie's own average.
    pub prob_below_own_average: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeakSpots {
    pub net_area: Vec<ShrunkSaves<NetArea>>,
    pub body_area: Vec<ShrunkSaves<BodyArea>>,
    pub distance: Vec<ShrunkSaves<ShotDistance>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalieTrendPoint {
    pub date: Date,
    pub opponent: String,
    pub shots_against: u32,
    pub saves: u32,
    pub save_pct: Option<f64>,
    pub instat_index: Option<f64>,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalieSeason {
    pub player: PlayerRef,
    pub games: u32,
    pub toi: Seconds,
    pub shots_against: u32,
    pub saves: u32,
    pub goals_against: u32,
    pub save_pct: Option<Estimate>,
    pub even_strength_save_pct: Option<Estimate>,
    pub short_handed_save_pct: Option<Estimate>,
    pub goals_against_average: Option<f64>,
    pub instat_mean: Option<f64>,
    pub trend: Vec<GoalieTrendPoint>,
    pub focus_details: Vec<StatEntry>,
    /// Shots and saves by distance, shot type, net area …, summed over games.
    pub splits: SaveSplits,
    /// Summed over the games whose page reported it.
    pub rebounds: Option<ReboundControl>,
    pub goals_saved: Option<GoalsSaved>,
    pub weak_spots: WeakSpots,
}

fn shrunk_splits<K: Copy>(splits: &[Saves<K>]) -> Vec<ShrunkSaves<K>> {
    let observations: Vec<(f64, f64)> = splits.iter().map(|x| (f64::from(x.saves), f64::from(x.shots))).collect();
    let Some(prior) = fit_beta_prior(&observations) else {
        return Vec::new();
    };
    splits
        .iter()
        .filter(|x| x.shots > 0)
        .filter_map(|x| {
            let save_pct = shrink(f64::from(x.saves), f64::from(x.shots - x.saves.min(x.shots)), prior)?;
            Some(ShrunkSaves {
                kind: x.kind,
                shots: x.shots,
                saves: x.saves,
                prob_below_own_average: 1.0 - save_pct.prob_above_average,
                save_pct,
            })
        })
        .collect()
}

/// Each goalie's share of a game's shots against, by the shots on goal InStat credits them with.
fn shares_of_game(goalies: &[(PlayerId, u32)]) -> Vec<(PlayerId, f64)> {
    let total: u32 = goalies.iter().map(|(_, shots)| shots).sum();
    goalies
        .iter()
        .map(|(id, shots)| {
            let share = if total == 0 { 1.0 / goalies.len() as f64 } else { f64::from(*shots) / f64::from(total) };
            (id.clone(), share)
        })
        .collect()
}

fn goalies_in_net(game: &Game) -> Vec<(PlayerId, u32)> {
    game.players
        .iter()
        .filter_map(|p| p.goalie.as_ref().filter(|g| g.toi.0 > 0.0).map(|g| (p.id.clone(), g.shots_against)))
        .collect()
}

fn add_periods(periods: &mut Vec<PeriodSaves>, context: &Context<'_>, game: &Game) {
    for (i, shot) in game.charted_shots_against.iter().enumerate() {
        let xg = context.shot_xg.theirs(&game.id, i).unwrap_or_default();
        let index = periods.iter().position(|p| p.period == shot.period).unwrap_or_else(|| {
            periods.push(PeriodSaves { period: shot.period, attempts: 0, goals: 0, xg_against: 0.0, saved_above_expected: 0.0 });
            periods.len() - 1
        });
        let row = &mut periods[index];
        row.attempts += 1;
        row.goals += u32::from(shot.goal);
        row.xg_against += xg;
        row.saved_above_expected += xg - f64::from(u8::from(shot.goal));
    }
}

fn goals_saved(context: &Context<'_>, id: &PlayerId, toi: Seconds) -> Option<GoalsSaved> {
    let mut xg_against = 0.0;
    let mut goals_against = 0;
    let mut shared_games = 0;
    let mut all_charted_and_alone = true;
    let mut games_counted = 0;
    let mut chances: Vec<f64> = Vec::new();
    let mut by_period: Vec<PeriodSaves> = Vec::new();
    for game in &context.scope {
        let in_net = goalies_in_net(game);
        let Some(share) = shares_of_game(&in_net).into_iter().find(|(g, _)| g == id).map(|(_, s)| s) else {
            continue;
        };
        let Some(stats) = game.player(id).and_then(|p| p.goalie.as_ref()) else {
            continue;
        };
        let charted: Option<Vec<f64>> = (0..game.charted_shots_against.len())
            .map(|i| context.shot_xg.theirs(&game.id, i))
            .collect();
        let charted = charted.filter(|c| !c.is_empty());
        let Some(game_xg) = charted.as_ref().map(|c| c.iter().sum()).or(game.opponent_summary.xg) else {
            all_charted_and_alone = false;
            continue;
        };
        games_counted += 1;
        xg_against += game_xg * share;
        goals_against += stats.goals_against;
        match (in_net.len(), charted) {
            (1, Some(values)) => {
                chances.extend(values);
                add_periods(&mut by_period, context, game);
            }
            (1, None) => all_charted_and_alone = false,
            _ => {
                shared_games += 1;
                all_charted_and_alone = false;
            }
        }
    }
    if games_counted == 0 {
        return None;
    }
    by_period.sort_by_key(|p| p.period);
    let saved_above_expected = xg_against - f64::from(goals_against);
    let chance_average_does_as_well = (all_charted_and_alone && !chances.is_empty()).then(|| {
        let allowed = usize::try_from(goals_against).unwrap_or(usize::MAX);
        goals_from_shots(&chances).iter().take(allowed.saturating_add(1)).sum::<f64>()
    });
    Some(GoalsSaved {
        xg_against,
        goals_against,
        saved_above_expected,
        per_60: per_60(saved_above_expected, toi),
        chance_average_does_as_well,
        shared_games,
        by_period,
    })
}

fn save_interval(saves: u32, shots: u32) -> Option<Estimate> {
    let (s, n) = (f64::from(saves), f64::from(shots));
    wilson_interval(s, n, 0.95).map(|(low, high)| Estimate {
        value: 100.0 * s / n,
        low: 100.0 * low,
        high: 100.0 * high,
    })
}

fn total_splits<'a>(stats: impl Iterator<Item = &'a GoalieStats>) -> SaveSplits {
    let mut totals = SaveSplits::default();
    for s in stats {
        totals.add(&s.splits);
    }
    totals
}

fn total_rebounds<'a>(stats: impl Iterator<Item = &'a GoalieStats>) -> Option<ReboundControl> {
    stats.filter_map(|s| s.rebounds.as_ref()).fold(None, |total, r| {
        let mut sum = total.unwrap_or_default();
        sum.add(r);
        Some(sum)
    })
}

#[must_use]
pub fn goalies(context: &Context<'_>) -> Vec<GoalieSeason> {
    let mut ids: Vec<PlayerId> = context
        .scope
        .iter()
        .flat_map(|g| g.players.iter().filter(|p| p.goalie.is_some()).map(|p| p.id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    ids.iter()
        .filter_map(|id| {
            let player = context.roster.get(id)?.clone();
            let appearances: Vec<_> = context
                .scope
                .iter()
                .filter_map(|g| g.player(id).and_then(|p| Some((*g, p, p.goalie.as_ref()?))))
                .filter(|(_, _, s)| s.toi.0 > 0.0)
                .collect();
            let games = u32::try_from(appearances.len()).ok()?;
            let toi: Seconds = appearances.iter().map(|(_, _, s)| s.toi).sum();
            let shots_against: u32 = appearances.iter().map(|(_, _, s)| s.shots_against).sum();
            let saves: u32 = appearances.iter().map(|(_, _, s)| s.saves).sum();
            let goals_against: u32 = appearances.iter().map(|(_, _, s)| s.goals_against).sum();
            let split = |pick: fn(&GoalieStats) -> Option<(u32, u32)>| {
                let (faced, stopped) = appearances
                    .iter()
                    .filter_map(|(_, _, s)| pick(s))
                    .fold((0, 0), |(a, b), (x, y)| (a + x, b + y));
                save_interval(stopped, faced)
            };
            let instat: Vec<f64> = appearances.iter().filter_map(|(_, _, s)| s.instat_index).collect();
            let mut trend: Vec<GoalieTrendPoint> = appearances
                .iter()
                .map(|(g, _, s)| GoalieTrendPoint {
                    date: g.date,
                    opponent: g.opponent.0.clone(),
                    shots_against: s.shots_against,
                    saves: s.saves,
                    save_pct: (s.shots_against > 0).then(|| 100.0 * f64::from(s.saves) / f64::from(s.shots_against)),
                    instat_index: s.instat_index,
                    loaded: true,
                })
                .collect();
            if let Some((_, latest, _)) = appearances.iter().max_by_key(|(g, _, _)| g.date) {
                for row in &latest.history {
                    let HistoryKind::Goalie { shots_against, saves, .. } = row.kind else {
                        continue;
                    };
                    if trend.iter().any(|t| (t.date.ordinal() - row.date.ordinal()).abs() <= 1) {
                        continue;
                    }
                    trend.push(GoalieTrendPoint {
                        date: row.date,
                        opponent: row.opponent.clone(),
                        shots_against,
                        saves,
                        save_pct: (shots_against > 0).then(|| 100.0 * f64::from(saves) / f64::from(shots_against)),
                        instat_index: row.instat_index,
                        loaded: false,
                    });
                }
            }
            trend.sort_by_key(|t| t.date);
            let focus_details = context
                .focus
                .and_then(|f| f.player(id))
                .or_else(|| appearances.last().map(|(_, p, _)| *p))
                .map(|p| p.details.clone())
                .unwrap_or_default();
            let splits = total_splits(appearances.iter().map(|(_, _, s)| *s));
            Some(GoalieSeason {
                player,
                games,
                toi,
                shots_against,
                saves,
                goals_against,
                save_pct: save_interval(saves, shots_against),
                even_strength_save_pct: split(|s| s.even_strength),
                short_handed_save_pct: split(|s| s.short_handed),
                goals_against_average: (toi.0 > 0.0).then(|| f64::from(goals_against) * 3600.0 / toi.0),
                instat_mean: mean(&instat),
                trend,
                focus_details,
                weak_spots: WeakSpots {
                    net_area: shrunk_splits(&splits.net_area),
                    body_area: shrunk_splits(&splits.body_area),
                    distance: shrunk_splits(&splits.distance),
                },
                splits,
                rebounds: total_rebounds(appearances.iter().map(|(_, _, s)| *s)),
                goals_saved: goals_saved(context, id, toi),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_game_is_split_by_shots_faced() {
        let shares = shares_of_game(&[(PlayerId("A".into()), 30), (PlayerId("B".into()), 10)]);
        assert!((shares[0].1 - 0.75).abs() < 1e-12);
        assert!((shares[1].1 - 0.25).abs() < 1e-12);
    }

    #[test]
    fn a_goalie_who_faced_no_shots_on_goal_still_gets_an_even_share() {
        let shares = shares_of_game(&[(PlayerId("A".into()), 0), (PlayerId("B".into()), 0)]);
        assert!((shares[0].1 - 0.5).abs() < 1e-12);
    }

    #[test]
    fn one_goal_on_two_shots_is_not_called_a_weak_spot_with_confidence() {
        let splits = [
            Saves { kind: NetArea::TopLeft, shots: 2, saves: 1 },
            Saves { kind: NetArea::Middle, shots: 40, saves: 37 },
            Saves { kind: NetArea::BottomRight, shots: 30, saves: 28 },
        ];
        let shrunk = shrunk_splits(&splits);
        let top_left = shrunk.iter().find(|x| x.kind == NetArea::TopLeft).unwrap();
        let pooled = 100.0 * 66.0 / 72.0;
        assert!(top_left.save_pct.estimate.value > 50.0 && top_left.save_pct.estimate.value < pooled);
        assert!(top_left.prob_below_own_average < 0.9);
    }
}
