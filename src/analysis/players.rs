//! Per-player season and single-game numbers, rates, percentiles and trends.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{BetaPrior, Estimate, PlayerRef, Shrunk, per_60, share_pct, shrink};
use crate::analysis::stints::Stint;
use crate::model::{
    AreaBattles, Date, Game, GameId, HistoryKind, Player, PlayerId, Seconds, SkaterStats, StatEntry, Strength, ZoneShots,
    add_area_battles, add_zone_shots,
};
use crate::stats::describe::{mean, percentile_rank, sample_sd, wilson_interval};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SkaterTotals {
    pub games: u32,
    pub goals: u32,
    pub assists: u32,
    pub points: u32,
    pub plus_minus: i64,
    pub toi: Seconds,
    pub ev_toi: Seconds,
    pub pp_toi: Seconds,
    pub sh_toi: Seconds,
    pub penalty_minutes: Seconds,
    pub shots: u32,
    pub shots_on_goal: u32,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub hits: u32,
    pub hits_against: u32,
    pub faceoffs: u32,
    pub faceoffs_won: u32,
    pub blocked_shots: u32,
    pub puck_battles: u32,
    pub puck_battles_won: u32,
    pub puck_losses: u32,
    pub puck_recoveries: u32,
    pub entries: u32,
    pub passes: u32,
    pub xg: f64,
    pub on_ice_xg_for: f64,
    pub on_ice_xg_against: f64,
    /// Even-strength goals for/against while on the ice (from shifts).
    pub on_ice_goals_for: u32,
    pub on_ice_goals_against: u32,
    pub shot_zones: Vec<ZoneShots>,
    pub battle_areas: Vec<AreaBattles>,
}

impl SkaterTotals {
    pub(crate) fn add(&mut self, s: &SkaterStats) {
        self.games += 1;
        self.goals += s.goals;
        self.assists += s.assists;
        self.points += s.goals + s.assists;
        self.plus_minus += s.plus_minus;
        self.toi += s.toi;
        self.ev_toi += Seconds((s.toi.0 - s.pp_toi.0 - s.sh_toi.0).max(0.0));
        self.pp_toi += s.pp_toi;
        self.sh_toi += s.sh_toi;
        self.penalty_minutes += s.penalty_minutes;
        self.shots += s.shots;
        self.shots_on_goal += s.shots_on_goal;
        self.corsi_for += s.corsi_for;
        self.corsi_against += s.corsi_against;
        self.hits += s.hits;
        self.hits_against += s.hits_against;
        self.faceoffs += s.faceoffs;
        self.faceoffs_won += s.faceoffs_won;
        self.blocked_shots += s.blocked_shots;
        self.puck_battles += s.puck_battles;
        self.puck_battles_won += s.puck_battles_won;
        self.puck_losses += s.puck_losses;
        self.puck_recoveries += s.puck_recoveries;
        self.entries += s.entries;
        self.passes += s.passes;
        self.xg += s.xg.unwrap_or_default();
        self.on_ice_xg_for += s.on_ice_xg_for.unwrap_or_default();
        self.on_ice_xg_against += s.on_ice_xg_against.unwrap_or_default();
        add_zone_shots(&mut self.shot_zones, &s.shot_zones);
        add_area_battles(&mut self.battle_areas, &s.battle_areas);
    }
}

/// Rates per 60 minutes (all situations unless noted).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Rates {
    pub goals: Option<f64>,
    pub points: Option<f64>,
    pub shots: Option<f64>,
    pub xg: Option<f64>,
    pub passes: Option<f64>,
    pub entries: Option<f64>,
    pub recoveries: Option<f64>,
    pub losses: Option<f64>,
    pub battles_won: Option<f64>,
    pub blocks: Option<f64>,
    pub hits: Option<f64>,
    /// Even strength only.
    pub corsi_for: Option<f64>,
    pub corsi_against: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Shares {
    pub corsi_pct: Option<f64>,
    /// On-ice CF% minus team CF% while the player was off (even strength).
    pub corsi_rel: Option<f64>,
    pub xg_pct: Option<f64>,
    pub goals_pct: Option<f64>,
    pub battles_pct: Option<Estimate>,
    pub faceoff_pct: Option<Estimate>,
    pub shooting_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerGameRow {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub toi: Seconds,
    pub goals: u32,
    pub assists: u32,
    pub plus_minus: i64,
    pub shots: u32,
    pub corsi_pct: Option<f64>,
    pub instat_index: Option<f64>,
    pub xg: Option<f64>,
    pub passes: u32,
    pub battles_pct: Option<f64>,
}

/// A point on the long-run trend, from loaded games or InStat's recent-games table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrendPoint {
    pub date: Date,
    pub opponent: String,
    pub instat_index: Option<f64>,
    pub toi: Seconds,
    pub points: Option<u32>,
    pub shots: Option<u32>,
    pub plus_minus: Option<i64>,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub metric: String,
    pub game: f64,
    pub season_mean: f64,
    pub season_sd: Option<f64>,
    pub z: Option<f64>,
    /// Where this game ranks among the player's games in scope (0–100).
    pub percentile: Option<f64>,
    pub badge: Option<String>,
    pub higher_is_better: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerSeason {
    pub player: PlayerRef,
    pub group: Option<String>,
    pub totals: SkaterTotals,
    pub rates: Rates,
    pub shares: Shares,
    pub shrunk_corsi: Option<Shrunk>,
    pub instat_mean: Option<f64>,
    pub instat_sd: Option<f64>,
    pub percentiles: HashMap<String, f64>,
    pub games: Vec<PlayerGameRow>,
    pub trend: Vec<TrendPoint>,
    pub focus: Vec<Comparison>,
    pub focus_details: Vec<StatEntry>,
    pub qualified: bool,
}

fn game_row(game: &Game, stats: &SkaterStats) -> PlayerGameRow {
    PlayerGameRow {
        game: game.id.clone(),
        date: game.date,
        opponent: game.opponent.0.clone(),
        toi: stats.toi,
        goals: stats.goals,
        assists: stats.assists,
        plus_minus: stats.plus_minus,
        shots: stats.shots,
        corsi_pct: share_pct(f64::from(stats.corsi_for), f64::from(stats.corsi_against)),
        instat_index: stats.instat_index,
        xg: stats.xg,
        passes: stats.passes,
        battles_pct: (stats.puck_battles > 0)
            .then(|| 100.0 * f64::from(stats.puck_battles_won) / f64::from(stats.puck_battles)),
    }
}

fn rates(t: &SkaterTotals) -> Rates {
    let f = f64::from;
    Rates {
        goals: per_60(f(t.goals), t.toi),
        points: per_60(f(t.points), t.toi),
        shots: per_60(f(t.shots), t.toi),
        xg: per_60(t.xg, t.toi),
        passes: per_60(f(t.passes), t.toi),
        entries: per_60(f(t.entries), t.toi),
        recoveries: per_60(f(t.puck_recoveries), t.toi),
        losses: per_60(f(t.puck_losses), t.toi),
        battles_won: per_60(f(t.puck_battles_won), t.toi),
        blocks: per_60(f(t.blocked_shots), t.toi),
        hits: per_60(f(t.hits), t.toi),
        corsi_for: per_60(f(t.corsi_for), t.ev_toi),
        corsi_against: per_60(f(t.corsi_against), t.ev_toi),
    }
}

/// Team even-strength shot attempts (for, against) over the games the player played.
fn team_ev_corsi(games: &[&Game], id: &PlayerId) -> (f64, f64) {
    games
        .iter()
        .filter(|g| g.player(id).is_some_and(|p| p.skater.is_some()))
        .fold((0.0, 0.0), |(f, a), g| {
            (
                f + f64::from(g.summary.even_strength_shots.0),
                a + f64::from(g.opponent_summary.even_strength_shots.0),
            )
        })
}

fn shares(t: &SkaterTotals, team_ev: (f64, f64)) -> Shares {
    let f = f64::from;
    let corsi_pct = share_pct(f(t.corsi_for), f(t.corsi_against));
    let off = (team_ev.0 - f(t.corsi_for), team_ev.1 - f(t.corsi_against));
    let corsi_rel = match (corsi_pct, share_pct(off.0.max(0.0), off.1.max(0.0))) {
        (Some(on), Some(off)) => Some(on - off),
        _ => None,
    };
    let interval = |won: u32, total: u32| {
        wilson_interval(f(won), f(total), 0.95).map(|(low, high)| Estimate {
            value: 100.0 * f(won) / f(total),
            low: 100.0 * low,
            high: 100.0 * high,
        })
    };
    Shares {
        corsi_pct,
        corsi_rel,
        xg_pct: share_pct(t.on_ice_xg_for, t.on_ice_xg_against),
        goals_pct: share_pct(f(t.on_ice_goals_for), f(t.on_ice_goals_against)),
        battles_pct: interval(t.puck_battles_won, t.puck_battles),
        faceoff_pct: interval(t.faceoffs_won, t.faceoffs),
        shooting_pct: (t.shots_on_goal > 0).then(|| 100.0 * f(t.goals) / f(t.shots_on_goal)),
    }
}

fn trend(player_games: &[(&Game, &Player)]) -> Vec<TrendPoint> {
    let mut points: Vec<TrendPoint> = player_games
        .iter()
        .filter_map(|(game, player)| {
            let s = player.skater.as_ref()?;
            Some(TrendPoint {
                date: game.date,
                opponent: game.opponent.0.clone(),
                instat_index: s.instat_index,
                toi: s.toi,
                points: Some(s.goals + s.assists),
                shots: Some(s.shots),
                plus_minus: Some(s.plus_minus),
                loaded: true,
            })
        })
        .collect();
    let latest_history = player_games
        .iter()
        .filter(|(_, p)| !p.history.is_empty())
        .max_by_key(|(g, _)| g.date)
        .map(|(_, p)| &p.history);
    for row in latest_history.into_iter().flatten() {
        let already = points
            .iter()
            .any(|p| (p.date.ordinal() - row.date.ordinal()).abs() <= 1);
        if already {
            continue;
        }
        let (points_value, shots, plus_minus) = match &row.kind {
            HistoryKind::Skater {
                goals,
                assists,
                shots,
                plus_minus,
                ..
            } => (Some(goals + assists), Some(*shots), Some(*plus_minus)),
            HistoryKind::Goalie { .. } => (None, None, None),
        };
        points.push(TrendPoint {
            date: row.date,
            opponent: row.opponent.clone(),
            instat_index: row.instat_index,
            toi: row.toi,
            points: points_value,
            shots,
            plus_minus,
            loaded: false,
        });
    }
    points.sort_by_key(|p| p.date);
    points
}

struct FocusMetric {
    name: &'static str,
    higher_is_better: bool,
    value: fn(&SkaterStats) -> Option<f64>,
}

const FOCUS_METRICS: [FocusMetric; 9] = [
    FocusMetric { name: "InStat Index", higher_is_better: true, value: |s| s.instat_index },
    FocusMetric { name: "Time on ice (min)", higher_is_better: true, value: |s| Some(s.toi.minutes()) },
    FocusMetric { name: "Points", higher_is_better: true, value: |s| Some(f64::from(s.goals + s.assists)) },
    FocusMetric { name: "Shots", higher_is_better: true, value: |s| Some(f64::from(s.shots)) },
    FocusMetric { name: "CF%", higher_is_better: true, value: |s| share_pct(f64::from(s.corsi_for), f64::from(s.corsi_against)) },
    FocusMetric { name: "+/-", higher_is_better: true, value: |s| Some(s.plus_minus as f64) },
    FocusMetric { name: "Puck battles won", higher_is_better: true, value: |s| Some(f64::from(s.puck_battles_won)) },
    FocusMetric { name: "Puck losses", higher_is_better: false, value: |s| Some(f64::from(s.puck_losses)) },
    FocusMetric { name: "xG", higher_is_better: true, value: |s| s.xg },
];

fn focus_comparisons(focus: &SkaterStats, season: &[&SkaterStats]) -> Vec<Comparison> {
    FOCUS_METRICS
        .iter()
        .filter_map(|metric| {
            let game = (metric.value)(focus)?;
            let values: Vec<f64> = season.iter().filter_map(|s| (metric.value)(s)).collect();
            let season_mean = mean(&values)?;
            let season_sd = sample_sd(&values);
            let z = season_sd.filter(|sd| *sd > 0.0).map(|sd| (game - season_mean) / sd);
            let best = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let worst = values.iter().copied().fold(f64::INFINITY, f64::min);
            let badge = if values.len() >= 3 && game >= best && metric.higher_is_better {
                Some(format!("Best {} of the season", metric.name))
            } else if values.len() >= 3 && game <= worst && !metric.higher_is_better {
                Some(format!("Fewest {} this season", metric.name))
            } else {
                z.filter(|z| z.abs() >= 2.0).map(|z| {
                    format!("{} {:.1} SD {} usual", metric.name, z.abs(), if z > 0.0 { "above" } else { "below" })
                })
            };
            Some(Comparison {
                metric: metric.name.to_owned(),
                game,
                season_mean,
                season_sd,
                z,
                percentile: percentile_rank(&values, game),
                badge,
                higher_is_better: metric.higher_is_better,
            })
        })
        .collect()
}

fn on_ice_goals(stints: &[Stint], id: &PlayerId) -> (u32, u32) {
    stints
        .iter()
        .filter(|s| s.strength == Strength::Even && s.has(id))
        .fold((0, 0), |(f, a), s| (f + s.goals_for, a + s.goals_against))
}

type SeasonMetric = (&'static str, fn(&PlayerSeason) -> Option<f64>);

const PERCENTILE_METRICS: [SeasonMetric; 10] = [
    ("Points/60", |p| p.rates.points),
    ("Shots/60", |p| p.rates.shots),
    ("xG/60", |p| p.rates.xg),
    ("CF%", |p| p.shares.corsi_pct),
    ("CF% rel", |p| p.shares.corsi_rel),
    ("Passes/60", |p| p.rates.passes),
    ("Recoveries/60", |p| p.rates.recoveries),
    ("Battles won %", |p| p.shares.battles_pct.map(|e| e.value)),
    ("Blocks/60", |p| p.rates.blocks),
    ("InStat Index", |p| p.instat_mean),
];

#[must_use]
pub fn player_seasons(context: &Context<'_>, corsi_prior: Option<BetaPrior>) -> Vec<PlayerSeason> {
    let mut by_player: std::collections::BTreeMap<PlayerId, Vec<(&Game, &Player)>> = std::collections::BTreeMap::new();
    for game in &context.scope {
        for player in game.players.iter().filter(|p| p.skater.is_some()) {
            by_player.entry(player.id.clone()).or_default().push((game, player));
        }
    }
    let mut seasons: Vec<PlayerSeason> = by_player
        .into_iter()
        .filter_map(|(id, mut appearances)| {
            appearances.sort_by_key(|(g, _)| g.date);
            let player_ref = context.roster.get(&id)?.clone();
            let mut totals = SkaterTotals::default();
            for (_, player) in &appearances {
                if let Some(stats) = &player.skater {
                    totals.add(stats);
                }
            }
            let (gf, ga) = on_ice_goals(&context.stints, &id);
            totals.on_ice_goals_for = gf;
            totals.on_ice_goals_against = ga;
            let games: Vec<PlayerGameRow> = appearances
                .iter()
                .filter_map(|(g, p)| Some(game_row(g, p.skater.as_ref()?)))
                .collect();
            let instat: Vec<f64> = games.iter().filter_map(|g| g.instat_index).collect();
            let season_stats: Vec<&SkaterStats> = appearances.iter().filter_map(|(_, p)| p.skater.as_ref()).collect();
            let focus_player = context
                .focus
                .and_then(|focus| appearances.iter().find(|(g, _)| g.id == focus.id));
            let focus = focus_player
                .and_then(|(_, p)| p.skater.as_ref())
                .map(|s| focus_comparisons(s, &season_stats))
                .unwrap_or_default();
            let focus_details = focus_player
                .or_else(|| appearances.last())
                .map(|(_, p)| p.details.clone())
                .unwrap_or_default();
            let group = appearances.last().and_then(|(_, p)| p.group.clone());
            let team_ev = team_ev_corsi(&context.scope, &id);
            let shrunk_corsi = corsi_prior.and_then(|prior| {
                shrink(f64::from(totals.corsi_for), f64::from(totals.corsi_against), prior)
            });
            Some(PlayerSeason {
                player: player_ref,
                group,
                rates: rates(&totals),
                shares: shares(&totals, team_ev),
                shrunk_corsi,
                instat_mean: mean(&instat),
                instat_sd: sample_sd(&instat),
                percentiles: HashMap::new(),
                trend: trend(&appearances),
                games,
                focus,
                focus_details,
                qualified: totals.toi.0 >= context.min_toi.0,
                totals,
            })
        })
        .collect();
    assign_percentiles(&mut seasons);
    seasons.sort_by(|a, b| b.totals.toi.0.total_cmp(&a.totals.toi.0).then_with(|| a.player.id.cmp(&b.player.id)));
    seasons
}

fn assign_percentiles(seasons: &mut [PlayerSeason]) {
    for (name, metric) in PERCENTILE_METRICS {
        let values: Vec<f64> = seasons.iter().filter(|s| s.qualified).filter_map(metric).collect();
        let ranks: Vec<Option<f64>> = seasons
            .iter()
            .map(|s| if s.qualified { metric(s).and_then(|v| percentile_rank(&values, v)) } else { None })
            .collect();
        for (season, rank) in seasons.iter_mut().zip(ranks) {
            if let Some(rank) = rank {
                season.percentiles.insert(name.to_owned(), rank);
            }
        }
    }
}

/// CF% prior across qualified skaters, for shrinking individual CF%.
#[must_use]
pub fn corsi_prior(context: &Context<'_>) -> Option<BetaPrior> {
    let mut totals: std::collections::BTreeMap<PlayerId, (f64, f64, f64)> = std::collections::BTreeMap::new();
    for game in &context.scope {
        for player in &game.players {
            if let Some(s) = &player.skater {
                let entry = totals.entry(player.id.clone()).or_default();
                entry.0 += f64::from(s.corsi_for);
                entry.1 += f64::from(s.corsi_for + s.corsi_against);
                entry.2 += s.toi.0;
            }
        }
    }
    let observations: Vec<(f64, f64)> = totals
        .values()
        .filter(|(_, _, toi)| *toi >= context.min_toi.0)
        .map(|(cf, n, _)| (*cf, *n))
        .collect();
    crate::analysis::common::fit_beta_prior(&observations)
}
