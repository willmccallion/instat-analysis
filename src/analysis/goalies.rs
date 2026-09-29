//! Goaltender save percentages with intervals, splits and trends.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{Estimate, PlayerRef};
use crate::model::{Date, HistoryKind, PlayerId, Seconds, StatEntry};
use crate::stats::describe::{mean, wilson_interval};

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
}

fn save_interval(saves: u32, shots: u32) -> Option<Estimate> {
    let (s, n) = (f64::from(saves), f64::from(shots));
    wilson_interval(s, n, 0.95).map(|(low, high)| Estimate {
        value: 100.0 * s / n,
        low: 100.0 * low,
        high: 100.0 * high,
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
            let split = |pick: fn(&crate::model::GoalieStats) -> Option<(u32, u32)>| {
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
            })
        })
        .collect()
}
