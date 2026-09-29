//! Position rankings from a transparent composite of game stats, and "form": how each
//! player's recent games compare with their own usual level.
//!
//! Each stat is rated against same-position teammates (z-score), after pulling low-ice-time
//! values toward the position average. Stats are grouped into Offence, Defence and Puck play
//! and combined with the coach's weights (see [`rating_setup`](super::rating_setup)); the
//! result is shown on a 50 ± 10 scale.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::analysis::players::{PlayerSeason, SkaterTotals};
use crate::analysis::rating_setup::{Category, PositionWeights, RatingStat, RatingWeights};
use crate::model::{Date, GameId, PlayerId, Position};
use crate::stats::describe::{mean, sample_sd};

/// Ice time that counts as much as the position average when shrinking a player's stats.
const PRIOR_SECONDS: f64 = 10.0 * 60.0;
/// Single stats are capped so one extreme number cannot dominate a rating.
const Z_CAP: f64 = 3.0;
const RECENT_GAMES: usize = 3;
const MIN_BASELINE_GAMES: usize = 4;
const MIN_LOADED_GAMES_FOR_COMPOSITE_FORM: usize = 5;

/// One player's stats in the period being rated.
#[derive(Debug, Clone)]
pub struct RatingInput {
    pub player: PlayerRef,
    pub totals: SkaterTotals,
    /// Team even-strength shot attempts (for, against) in the same games.
    pub team_even_strength: (f64, f64),
    pub qualified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Component {
    pub metric: String,
    pub category: Category,
    /// How much it counts within its category (the coach's weight).
    pub weight: f64,
    pub value: Option<f64>,
    /// Standing vs same-position teammates, sign-adjusted so positive is always good.
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankingRow {
    pub player: PlayerRef,
    /// 1 = best among qualified players at the position.
    pub rank: Option<usize>,
    /// 50 = position average; each 10 points is one standard deviation.
    pub rating: f64,
    pub offence: Option<f64>,
    pub defence: Option<f64>,
    pub puck_play: Option<f64>,
    pub components: Vec<Component>,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub toi: f64,
    pub games: u32,
    pub qualified: bool,
}

fn f(v: u32) -> f64 {
    f64::from(v)
}

/// Weighted mean of the `(value, weight)` pairs that have a value; `None` if none count.
fn weighted_mean(pairs: impl Iterator<Item = (Option<f64>, f64)>) -> Option<f64> {
    let (sum, total) = pairs
        .filter_map(|(value, weight)| Some((value?, weight)))
        .filter(|(_, weight)| *weight > 0.0)
        .fold((0.0, 0.0), |(sum, total), (value, weight)| (sum + value * weight, total + weight));
    (total > 0.0).then(|| sum / total)
}

/// Rates one position group. Only qualified players set the averages, but everyone is rated.
#[must_use]
pub fn rate(inputs: &[RatingInput], position: Position, weights: &PositionWeights) -> Vec<RankingRow> {
    let metrics: Vec<RatingStat> = RatingStat::ALL.into_iter().filter(|s| weights.stat(*s) > 0.0).collect();
    let group: Vec<&RatingInput> = inputs.iter().filter(|i| i.player.position == position).collect();
    let reference: Vec<&RatingInput> = {
        let qualified: Vec<&RatingInput> = group.iter().copied().filter(|i| i.qualified).collect();
        if qualified.len() >= 3 { qualified } else { group.clone() }
    };
    let stats: Vec<(Option<f64>, Option<f64>)> = metrics
        .iter()
        .map(|m| {
            let values: Vec<f64> = reference.iter().filter_map(|i| m.value(i)).collect();
            (mean(&values), sample_sd(&values))
        })
        .collect();
    let mut rows: Vec<RankingRow> = group
        .iter()
        .map(|input| {
            let weight = input.totals.toi.0 / (input.totals.toi.0 + PRIOR_SECONDS);
            let components: Vec<Component> = metrics
                .iter()
                .zip(&stats)
                .map(|(m, (average, sd))| {
                    let value = m.value(input);
                    let score = match (value, average, sd) {
                        (Some(v), Some(avg), Some(sd)) if *sd > 0.0 => {
                            let shrunk = weight * v + (1.0 - weight) * avg;
                            let z = ((shrunk - avg) / sd).clamp(-Z_CAP, Z_CAP);
                            Some(if m.higher_is_better() { z } else { -z })
                        }
                        (Some(_), Some(_), _) => Some(0.0),
                        _ => None,
                    };
                    Component {
                        metric: m.name().to_owned(),
                        category: m.category(),
                        weight: weights.stat(*m),
                        value,
                        score,
                    }
                })
                .collect();
            let category = |c: Category| weighted_mean(components.iter().filter(|x| x.category == c).map(|x| (x.score, x.weight)));
            let (offence, defence, puck_play) = (category(Category::Offence), category(Category::Defence), category(Category::PuckPlay));
            let overall = weighted_mean(
                [(offence, Category::Offence), (defence, Category::Defence), (puck_play, Category::PuckPlay)]
                    .into_iter()
                    .map(|(score, c)| (score, weights.category(c))),
            )
            .unwrap_or(0.0);
            let mut ordered: Vec<&Component> = components.iter().filter(|c| c.score.is_some()).collect();
            ordered.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
            let strengths = ordered.iter().take(2).filter(|c| c.score.unwrap_or(0.0) > 0.25).map(|c| c.metric.clone()).collect();
            let weaknesses = ordered.iter().rev().take(2).filter(|c| c.score.unwrap_or(0.0) < -0.25).map(|c| c.metric.clone()).collect();
            RankingRow {
                player: input.player.clone(),
                rank: None,
                rating: 50.0 + 10.0 * overall,
                offence: offence.map(|v| 50.0 + 10.0 * v),
                defence: defence.map(|v| 50.0 + 10.0 * v),
                puck_play: puck_play.map(|v| 50.0 + 10.0 * v),
                components,
                strengths,
                weaknesses,
                toi: input.totals.toi.0,
                games: input.totals.games,
                qualified: input.qualified,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.qualified
            .cmp(&a.qualified)
            .then(b.rating.total_cmp(&a.rating))
            .then_with(|| a.player.id.cmp(&b.player.id))
    });
    for (rank, row) in (1..).zip(rows.iter_mut().filter(|r| r.qualified)) {
        row.rank = Some(rank);
    }
    rows
}

#[must_use]
pub fn season_inputs(context: &Context<'_>, seasons: &[PlayerSeason]) -> Vec<RatingInput> {
    seasons
        .iter()
        .filter(|s| matches!(s.player.position, Position::Forward | Position::Defence))
        .map(|s| RatingInput {
            player: s.player.clone(),
            totals: s.totals.clone(),
            team_even_strength: context
                .scope
                .iter()
                .filter(|g| g.player(&s.player.id).is_some())
                .fold((0.0, 0.0), |(a, b), g| {
                    (a + f(g.summary.even_strength_shots.0), b + f(g.opponent_summary.even_strength_shots.0))
                }),
            qualified: s.qualified,
        })
        .collect()
}

/// Ratings within each single game (every player who dressed counts as qualified).
fn game_ratings(context: &Context<'_>, weights: &RatingWeights) -> BTreeMap<PlayerId, Vec<(GameId, Date, f64)>> {
    let mut result: BTreeMap<PlayerId, Vec<(GameId, Date, f64)>> = BTreeMap::new();
    for game in &context.scope {
        let team = (f(game.summary.even_strength_shots.0), f(game.opponent_summary.even_strength_shots.0));
        let inputs: Vec<RatingInput> = game
            .players
            .iter()
            .filter_map(|p| {
                let stats = p.skater.as_ref()?;
                let player = context.roster.get(&p.id)?.clone();
                let mut totals = SkaterTotals::default();
                totals.add(stats);
                Some(RatingInput {
                    player,
                    totals,
                    team_even_strength: team,
                    qualified: true,
                })
            })
            .collect();
        for (position, position_weights) in [(Position::Forward, &weights.forwards), (Position::Defence, &weights.defence)] {
            for row in rate(&inputs, position, position_weights) {
                result
                    .entry(row.player.id.clone())
                    .or_default()
                    .push((game.id.clone(), game.date, row.rating));
            }
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FormSource {
    /// This app's composite rating, game by game (5+ loaded games).
    CompositeRating,
    /// InStat Index from loaded games plus the Player report's recent-games table.
    InstatIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormRow {
    pub player: PlayerRef,
    pub source: FormSource,
    pub baseline_games: usize,
    pub baseline_mean: f64,
    pub baseline_sd: f64,
    pub recent_games: usize,
    pub recent_mean: f64,
    /// (recent − usual) in the player's own standard deviations.
    pub recent_z: f64,
    pub latest_date: Date,
    pub latest_value: f64,
    pub latest_z: f64,
    /// (date, value) for sparklines, oldest first.
    pub series: Vec<(Date, f64)>,
}

fn form_row(player: PlayerRef, source: FormSource, series: Vec<(Date, f64)>, latest_index: usize) -> Option<FormRow> {
    if series.len() < MIN_BASELINE_GAMES + 1 {
        return None;
    }
    let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
    let recent_end = (latest_index + 1).min(values.len());
    let recent_start = recent_end.saturating_sub(RECENT_GAMES);
    let recent = &values[recent_start..recent_end];
    let baseline: Vec<f64> = values
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < recent_start || *i >= recent_end)
        .map(|(_, v)| *v)
        .collect();
    if baseline.len() < MIN_BASELINE_GAMES {
        return None;
    }
    let baseline_mean = mean(&baseline)?;
    let baseline_sd = sample_sd(&baseline)?;
    if baseline_sd <= 0.0 {
        return None;
    }
    let recent_mean = mean(recent)?;
    let (latest_date, latest_value) = series[latest_index];
    Some(FormRow {
        player,
        source,
        baseline_games: baseline.len(),
        baseline_mean,
        baseline_sd,
        recent_games: recent.len(),
        recent_mean,
        recent_z: (recent_mean - baseline_mean) / baseline_sd,
        latest_date,
        latest_value,
        latest_z: (latest_value - baseline_mean) / baseline_sd,
        series,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankingsReport {
    pub forwards: Vec<RankingRow>,
    pub defence: Vec<RankingRow>,
    /// Best to worst recent form (last few games vs usual).
    pub form: Vec<FormRow>,
    pub recent_window: usize,
    pub composite_form_needs: usize,
}

#[must_use]
pub fn rankings(context: &Context<'_>, seasons: &[PlayerSeason], weights: &RatingWeights) -> RankingsReport {
    let inputs = season_inputs(context, seasons);
    let per_game = game_ratings(context, weights);
    let mut form: Vec<FormRow> = seasons
        .iter()
        .filter(|s| matches!(s.player.position, Position::Forward | Position::Defence))
        .filter_map(|s| {
            let loaded = per_game.get(&s.player.id).map_or(&[][..], Vec::as_slice);
            let focus_index = |series: &[(Date, f64)], date: Option<Date>| {
                date.and_then(|d| series.iter().position(|(sd, _)| *sd == d)).unwrap_or_else(|| series.len().saturating_sub(1))
            };
            let focus_date = context.focus.map(|g| g.date);
            if loaded.len() >= MIN_LOADED_GAMES_FOR_COMPOSITE_FORM {
                let series: Vec<(Date, f64)> = loaded.iter().map(|(_, d, r)| (*d, *r)).collect();
                let index = focus_index(&series, focus_date);
                form_row(s.player.clone(), FormSource::CompositeRating, series, index)
            } else {
                let series: Vec<(Date, f64)> = s.trend.iter().filter_map(|t| Some((t.date, t.instat_index?))).collect();
                let index = focus_index(&series, focus_date);
                form_row(s.player.clone(), FormSource::InstatIndex, series, index)
            }
        })
        .collect();
    form.sort_by(|a, b| b.recent_z.total_cmp(&a.recent_z).then_with(|| a.player.id.cmp(&b.player.id)));
    RankingsReport {
        forwards: rate(&inputs, Position::Forward, &weights.forwards),
        defence: rate(&inputs, Position::Defence, &weights.defence),
        form,
        recent_window: RECENT_GAMES,
        composite_form_needs: MIN_LOADED_GAMES_FOR_COMPOSITE_FORM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Seconds;

    fn input(name: &str, points: u32, shots: u32, toi_minutes: f64) -> RatingInput {
        RatingInput {
            player: PlayerRef {
                id: PlayerId(name.to_owned()),
                name: name.to_owned(),
                jersey: None,
                position: Position::Forward,
            },
            totals: SkaterTotals {
                games: 1,
                points,
                shots,
                toi: Seconds(toi_minutes * 60.0),
                ev_toi: Seconds(toi_minutes * 60.0),
                corsi_for: 10,
                corsi_against: 10,
                puck_battles: 10,
                puck_battles_won: 5,
                ..SkaterTotals::default()
            },
            team_even_strength: (50.0, 50.0),
            qualified: true,
        }
    }

    #[test]
    fn better_offence_ranks_higher() {
        let rows = rate(&[input("a", 0, 1, 15.0), input("b", 3, 8, 15.0), input("c", 1, 4, 15.0)], Position::Forward, &RatingWeights::default().forwards);
        let order: Vec<&str> = rows.iter().map(|r| r.player.name.as_str()).collect();
        assert_eq!(order, vec!["b", "c", "a"]);
        assert_eq!(rows[0].rank, Some(1));
        assert!(rows[0].rating > 50.0 && rows[2].rating < 50.0);
    }

    #[test]
    fn low_ice_time_is_pulled_toward_average() {
        let full = rate(&[input("a", 0, 1, 15.0), input("b", 3, 8, 15.0), input("c", 1, 4, 15.0)], Position::Forward, &RatingWeights::default().forwards);
        let short = rate(&[input("a", 0, 1, 15.0), input("b", 3, 8, 15.0), input("c", 1, 4, 15.0), input("d", 1, 3, 2.0)], Position::Forward, &RatingWeights::default().forwards);
        let d = short.iter().find(|r| r.player.name == "d").unwrap();
        let offence_short = d.offence.unwrap();
        let top = full.iter().map(|r| r.offence.unwrap()).fold(f64::MIN, f64::max);
        assert!(offence_short < top, "a 2-minute player should not top the list on rates alone");
    }

    /// `(stat, weight)` pairs for one position, with the given category weights.
    fn weights(categories: &[(Category, f64)], stats: &[(RatingStat, f64)]) -> PositionWeights {
        let json = serde_json::json!({
            "categories": categories.iter().map(|(c, w)| (serde_json::to_value(c).unwrap().as_str().unwrap().to_owned(), *w)).collect::<BTreeMap<_, _>>(),
            "stats": stats.iter().map(|(s, w)| (serde_json::to_value(s).unwrap().as_str().unwrap().to_owned(), *w)).collect::<BTreeMap<_, _>>(),
        });
        serde_json::from_value(json).unwrap()
    }

    fn two_way(name: &str, points: u32, blocks: u32) -> RatingInput {
        let mut i = input(name, points, 5, 15.0);
        i.totals.blocked_shots = blocks;
        i
    }

    #[test]
    fn a_stat_weighted_zero_is_left_out() {
        let only_shots = weights(&[(Category::Offence, 1.0)], &[(RatingStat::ShotsPer60, 1.0), (RatingStat::PointsPer60, 0.0)]);

        let rows = rate(&[input("a", 5, 1, 15.0), input("b", 0, 8, 15.0), input("c", 2, 4, 15.0)], Position::Forward, &only_shots);

        assert_eq!(rows[0].player.name, "b");
        assert!(rows[0].components.iter().all(|c| c.metric != "Points/60"));
    }

    #[test]
    fn category_weights_decide_between_a_scorer_and_a_shot_blocker() {
        let players = [two_way("scorer", 4, 0), two_way("blocker", 0, 6), two_way("middle", 2, 3)];
        let stats = [(RatingStat::PointsPer60, 1.0), (RatingStat::BlocksPer60, 1.0)];
        let top = |offence: f64, defence: f64| {
            let w = weights(&[(Category::Offence, offence), (Category::Defence, defence)], &stats);
            rate(&players, Position::Forward, &w)[0].player.name.clone()
        };

        assert_eq!(top(3.0, 1.0), "scorer");
        assert_eq!(top(1.0, 3.0), "blocker");
    }

    #[test]
    fn form_compares_recent_games_to_the_players_own_baseline() {
        let date = |d| Date { year: 2026, month: 1, day: d };
        let series: Vec<(Date, f64)> = [100.0, 102.0, 98.0, 101.0, 99.0, 80.0, 82.0, 78.0]
            .iter()
            .enumerate()
            .map(|(i, v)| (date(u8::try_from(i + 1).unwrap()), *v))
            .collect();
        let row = form_row(input("a", 0, 0, 10.0).player, FormSource::InstatIndex, series, 7).unwrap();
        assert_eq!(row.recent_games, 3);
        assert_eq!(row.baseline_games, 5);
        assert!(row.recent_z < -5.0, "{}", row.recent_z);
    }
}
