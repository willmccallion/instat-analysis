//! Real changes in form: the single point in each player's InStat Index history where their
//! level shifted most, and a permutation test of whether a shift that big is more than
//! game-to-game noise.

use serde::Serialize;

use crate::analysis::common::{PlayerRef, TestRow, Verdict};
use crate::analysis::players::PlayerSeason;
use crate::model::Date;
use crate::stats::random;

const MIN_GAMES: usize = 8;
/// Games on each side of a change, so one odd game can't be a "change".
const MIN_SEGMENT: usize = 3;
const PERMUTATIONS: usize = 999;
/// A gap this long between games is an off-season, where a change of level is expected.
const BREAK_DAYS: i64 = 60;
/// Changes this close together for several players point at the schedule, not one player.
const SAME_TIME_DAYS: i64 = 14;
const SAME_TIME_PLAYERS: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormChange {
    pub player: PlayerRef,
    /// First game after the change.
    pub from: Date,
    pub before_games: usize,
    pub before_mean: f64,
    pub after_games: usize,
    pub after_mean: f64,
    /// Share of shuffled orders of the same games with a split at least this clear.
    pub p: f64,
    /// The change falls across an off-season break.
    pub after_break: bool,
    /// Other players changed within two weeks of this date too (the schedule, not the player).
    pub team_wide: bool,
}

/// Where splitting `values` into before/after explains the most spread, and how much it
/// explains (the between-segment sum of squares).
fn best_split(values: &[f64]) -> Option<(usize, f64)> {
    let n = values.len();
    if n < 2 * MIN_SEGMENT {
        return None;
    }
    let total: f64 = values.iter().sum();
    let mut prefix = 0.0;
    let mut best: Option<(usize, f64)> = None;
    for (k, v) in values.iter().enumerate().take(n - MIN_SEGMENT) {
        prefix += v;
        let left = k + 1;
        if left < MIN_SEGMENT {
            continue;
        }
        let right = n - left;
        let (m1, m2) = (prefix / left as f64, (total - prefix) / right as f64);
        let between = left as f64 * right as f64 / n as f64 * (m1 - m2).powi(2);
        if best.is_none_or(|(_, b)| between > b) {
            best = Some((left, between));
        }
    }
    best
}

fn permutation_p(values: &[f64], observed: f64, seed: u64) -> f64 {
    let mut rng = random::seeded(seed);
    let mut shuffled = values.to_vec();
    let at_least = (0..PERMUTATIONS)
        .filter(|_| {
            random::shuffle(&mut rng, &mut shuffled);
            best_split(&shuffled).is_some_and(|(_, b)| b >= observed - 1e-12)
        })
        .count();
    (at_least + 1) as f64 / (PERMUTATIONS + 1) as f64
}

#[must_use]
pub fn changes(seasons: &[PlayerSeason], tests: &mut Vec<TestRow>) -> Vec<FormChange> {
    let family = "Form changes";
    let mut result = Vec::new();
    for (seed, season) in (0..).zip(seasons) {
        let series: Vec<(Date, f64)> = season.trend.iter().filter_map(|t| Some((t.date, t.instat_index?))).collect();
        if series.len() < MIN_GAMES {
            continue;
        }
        let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
        let Some((split, between)) = best_split(&values) else {
            continue;
        };
        let p = permutation_p(&values, between, 1000 + seed);
        let mean = |part: &[f64]| part.iter().sum::<f64>() / part.len() as f64;
        let change = FormChange {
            player: season.player.clone(),
            from: series[split].0,
            before_games: split,
            before_mean: mean(&values[..split]),
            after_games: values.len() - split,
            after_mean: mean(&values[split..]),
            p,
            after_break: series[split].0.ordinal() - series[split - 1].0.ordinal() > BREAK_DAYS,
            team_wide: false,
        };
        tests.push(TestRow {
            family: family.to_owned(),
            question: format!("Did {}'s level change around {}?", season.player.name, change.from),
            method: format!("Best single split of the InStat Index series, permutation test ({PERMUTATIONS} shuffles)"),
            statistic_label: "Between-segment sum of squares".to_owned(),
            statistic: Some(between),
            df: None,
            p: Some(p),
            p_adjusted: None,
            effect_label: "After minus before (InStat Index)".to_owned(),
            effect: Some(change.after_mean - change.before_mean),
            ci: None,
            n: format!("{} games", values.len()),
            secondary: None,
            verdict: Verdict::from_adjusted_p(Some(p)),
            plain: format!(
                "InStat Index averaged {:.0} over {} games, then {:.0} over the next {}.",
                change.before_mean, change.before_games, change.after_mean, change.after_games
            ),
            assumptions: "Games are exchangeable if nothing changed; one change at most.".to_owned(),
        });
        result.push(change);
    }
    mark_team_wide(&mut result);
    result.sort_by(|a, b| a.p.total_cmp(&b.p));
    result
}

/// Flags clear changes that several players share around the same date.
fn mark_team_wide(changes: &mut [FormChange]) {
    let clear: Vec<i64> = changes.iter().filter(|c| c.p < 0.05).map(|c| c.from.ordinal()).collect();
    for change in changes.iter_mut() {
        let day = change.from.ordinal();
        let together = clear.iter().filter(|d| (**d - day).abs() <= SAME_TIME_DAYS).count();
        change.team_wide = together >= SAME_TIME_PLAYERS;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_lands_where_the_level_jumps() {
        let values = [100.0, 102.0, 98.0, 101.0, 80.0, 79.0, 82.0, 81.0];
        let (split, _) = best_split(&values).unwrap();
        assert_eq!(split, 4);
        let p = permutation_p(&values, best_split(&values).unwrap().1, 1);
        assert!(p < 0.05, "{p}");
    }

    #[test]
    fn noise_alone_is_not_called_a_change() {
        let values = [100.0, 90.0, 110.0, 95.0, 105.0, 98.0, 102.0, 93.0, 107.0];
        let p = permutation_p(&values, best_split(&values).unwrap().1, 2);
        assert!(p > 0.2, "{p}");
    }

    fn change(day: u8, p: f64) -> FormChange {
        FormChange {
            player: PlayerRef { id: crate::model::PlayerId(format!("P{day}")), name: String::new(), jersey: None, position: crate::model::Position::Forward },
            from: Date { year: 2026, month: 2, day },
            before_games: 5,
            before_mean: 100.0,
            after_games: 5,
            after_mean: 80.0,
            p,
            after_break: false,
            team_wide: false,
        }
    }

    #[test]
    fn changes_shared_by_several_players_are_marked_team_wide() {
        let mut changes = vec![change(1, 0.01), change(5, 0.02), change(12, 0.03), change(28, 0.01)];
        mark_team_wide(&mut changes);
        assert!(changes[0].team_wide && changes[1].team_wide && changes[2].team_wide);
        assert!(!changes[3].team_wide, "a lone change later on is the player's own");
    }

    #[test]
    fn no_split_leaves_fewer_than_three_games_on_a_side() {
        assert!(best_split(&[1.0, 2.0, 3.0, 4.0, 5.0]).is_none());
        let (split, _) = best_split(&[0.0, 0.0, 0.0, 0.0, 0.0, 9.0]).unwrap();
        assert_eq!(split, 3);
    }
}
