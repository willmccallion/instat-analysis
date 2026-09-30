//! Line builder: every possible forward line and defence pair ranked by the shot-attempt
//! share the individual impact ratings predict, and the best full set of lines.
//!
//! Predictions add up each player's separate effect, so they cannot see chemistry, and for
//! combinations that have never played together they are extrapolations.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::analysis::impact::Ratings;
use crate::analysis::units::{UnitRow, UnitsReport};
use crate::model::{PlayerId, Position, UnitKind};

/// Faceoffs per game that mark a forward as able to play centre.
const CENTRE_FACEOFFS_PER_GAME: f64 = 3.0;
const FORWARD_LINES: usize = 4;
const DEFENCE_PAIRS: usize = 3;
const SHOWN_COMBOS: usize = 15;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Combo {
    pub players: Vec<PlayerRef>,
    /// Shot-attempt share the ratings predict, percent.
    pub expected_pct: f64,
    /// Even-strength minutes this exact group played together in the games in scope.
    pub minutes_together: f64,
    pub observed_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lineup {
    pub units: Vec<Combo>,
    /// Predicted share with these units given the ice time the current units get, the best
    /// unit taking the most.
    pub expected_pct: f64,
    /// Predicted share of the current units (the most-used ones), with their ice time.
    pub current_pct: Option<f64>,
    /// Share of the minutes each slot gets, most first.
    pub minute_shares: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineGroup {
    pub best: Vec<Combo>,
    pub lineup: Option<Lineup>,
    /// Whether every forward line was required to include a faceoff taker.
    pub needs_centre: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineupReport {
    pub games: usize,
    pub forwards: Option<LineGroup>,
    pub defence: Option<LineGroup>,
}

fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    fn extend(start: usize, n: usize, k: usize, current: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if current.len() == k {
            out.push(current.clone());
            return;
        }
        for i in start..n {
            current.push(i);
            extend(i + 1, n, k, current, out);
            current.pop();
        }
    }
    let mut out = Vec::new();
    extend(0, n, k, &mut Vec::new(), &mut out);
    out
}

/// Predicted share of units playing `shares` of the minutes, the best unit taking the most.
fn slotted(values: &[f64], shares: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| b.total_cmp(a));
    sorted.iter().zip(shares).map(|(v, w)| v * w).sum()
}

/// Indexes of the set of `shares.len()` disjoint groups, each given as (members, predicted
/// share), with the best predicted share when the best group plays the most. Exhaustive
/// search, fine for a roster's worth of players.
fn best_partition(groups: &[(Vec<usize>, f64)], shares: &[f64]) -> Option<Vec<usize>> {
    fn search(groups: &[(Vec<usize>, f64)], shares: &[f64], from: usize, used: &mut Vec<bool>, chosen: &mut Vec<usize>, values: &mut Vec<f64>, best: &mut Option<(f64, Vec<usize>)>) {
        if chosen.len() == shares.len() {
            let score = slotted(values, shares);
            if best.as_ref().is_none_or(|(s, _)| score > *s + 1e-12) {
                *best = Some((score, chosen.clone()));
            }
            return;
        }
        for (index, (members, value)) in groups.iter().enumerate().skip(from) {
            if members.iter().any(|&m| used[m]) {
                continue;
            }
            for &m in members {
                used[m] = true;
            }
            chosen.push(index);
            values.push(*value);
            search(groups, shares, index + 1, used, chosen, values, best);
            values.pop();
            chosen.pop();
            for &m in members {
                used[m] = false;
            }
        }
    }
    let players = groups.iter().flat_map(|(m, _)| m.iter().copied()).max()? + 1;
    let mut best = None;
    search(groups, shares, 0, &mut vec![false; players], &mut Vec::new(), &mut Vec::new(), &mut best);
    best.map(|(_, chosen)| chosen)
}

fn sorted_ids(players: &[PlayerRef]) -> Vec<PlayerId> {
    let mut ids: Vec<PlayerId> = players.iter().map(|p| p.id.clone()).collect();
    ids.sort();
    ids
}

fn group(context: &Context<'_>, ratings: &Ratings, rows: &[UnitRow], position: Position, size: usize, units: usize, centres: &[PlayerId]) -> Option<LineGroup> {
    let mut players: Vec<PlayerRef> = ratings
        .offence
        .keys()
        .filter_map(|id| context.roster.get(id))
        .filter(|p| p.position == position)
        .cloned()
        .collect();
    players.sort_by(|a, b| a.id.cmp(&b.id));
    if players.len() < size {
        return None;
    }
    let observed: HashMap<Vec<PlayerId>, &UnitRow> = rows.iter().map(|r| (sorted_ids(&r.players), r)).collect();
    let needs_centre = size == 3 && centres.iter().filter(|c| players.iter().any(|p| &p.id == *c)).count() >= units;
    let combos: Vec<(Vec<usize>, Combo)> = combinations(players.len(), size)
        .into_iter()
        .filter_map(|members| {
            let ids: Vec<&PlayerId> = members.iter().map(|&i| &players[i].id).collect();
            let expected_pct = ratings.expected_share(&ids)?;
            let key: Vec<PlayerId> = {
                let mut k: Vec<PlayerId> = ids.iter().map(|id| (*id).clone()).collect();
                k.sort();
                k
            };
            let row = observed.get(&key);
            Some((members.clone(), Combo {
                players: members.iter().map(|&i| players[i].clone()).collect(),
                expected_pct,
                minutes_together: row.map_or(0.0, |r| r.toi.minutes()),
                observed_pct: row.and_then(|r| r.corsi_pct),
            }))
        })
        .collect();
    let mut best: Vec<Combo> = combos.iter().map(|(_, c)| c.clone()).collect();
    best.sort_by(|a, b| b.expected_pct.total_cmp(&a.expected_pct));
    best.truncate(SHOWN_COMBOS);
    let allowed: Vec<&(Vec<usize>, Combo)> = combos
        .iter()
        .filter(|(members, _)| !needs_centre || members.iter().any(|&i| centres.contains(&players[i].id)))
        .collect();
    let candidates: Vec<(Vec<usize>, f64)> = allowed.iter().map(|(members, c)| (members.clone(), c.expected_pct)).collect();
    let (shares, current_pct) = current_usage(ratings, rows, units);
    let lineup = (players.len() >= size * units)
        .then(|| best_partition(&candidates, &shares))
        .flatten()
        .map(|chosen| {
            let mut units_chosen: Vec<Combo> = chosen.iter().map(|&i| allowed[i].1.clone()).collect();
            units_chosen.sort_by(|a, b| b.expected_pct.total_cmp(&a.expected_pct));
            let values: Vec<f64> = units_chosen.iter().map(|c| c.expected_pct).collect();
            Lineup { expected_pct: slotted(&values, &shares), units: units_chosen, current_pct, minute_shares: shares.clone() }
        });
    Some(LineGroup { best, lineup, needs_centre })
}

/// The minute split of the `units` most-used current units (equal when fewer are known) and
/// their predicted share with those minutes.
fn current_usage(ratings: &Ratings, rows: &[UnitRow], units: usize) -> (Vec<f64>, Option<f64>) {
    let mut used: Vec<&UnitRow> = rows.iter().filter(|r| r.toi.0 > 0.0).collect();
    used.sort_by(|a, b| b.toi.0.total_cmp(&a.toi.0));
    used.truncate(units);
    let minutes: f64 = used.iter().map(|r| r.toi.0).sum();
    if used.len() < units || minutes <= 0.0 {
        return (vec![1.0 / units as f64; units], None);
    }
    let shares: Vec<f64> = used.iter().map(|r| r.toi.0 / minutes).collect();
    let predicted: Option<Vec<f64>> = used
        .iter()
        .map(|r| {
            let ids: Vec<&PlayerId> = r.players.iter().map(|p| &p.id).collect();
            ratings.expected_share(&ids)
        })
        .collect();
    let current = predicted.map(|p| p.iter().zip(&shares).map(|(v, w)| v * w).sum());
    (shares, current)
}

fn centres(context: &Context<'_>) -> Vec<PlayerId> {
    let mut faceoffs: HashMap<&PlayerId, (u32, u32)> = HashMap::new();
    for game in &context.scope {
        for player in &game.players {
            if let Some(s) = &player.skater {
                let entry = faceoffs.entry(&player.id).or_default();
                entry.0 += s.faceoffs;
                entry.1 += 1;
            }
        }
    }
    faceoffs
        .into_iter()
        .filter(|(_, (taken, games))| f64::from(*taken) / f64::from((*games).max(1)) >= CENTRE_FACEOFFS_PER_GAME)
        .map(|(id, _)| id.clone())
        .collect()
}

#[must_use]
pub(crate) fn lineup(context: &Context<'_>, ratings: &HashMap<UnitKind, Ratings>, units: &UnitsReport) -> LineupReport {
    let centres = centres(context);
    LineupReport {
        games: context.scope.len(),
        forwards: ratings
            .get(&UnitKind::ForwardLine)
            .and_then(|r| group(context, r, &units.forward_lines, Position::Forward, 3, FORWARD_LINES, &centres)),
        defence: ratings
            .get(&UnitKind::DefencePair)
            .and_then(|r| group(context, r, &units.defence_pairs, Position::Defence, 2, DEFENCE_PAIRS, &centres)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_count_matches_the_binomial_coefficient() {
        assert_eq!(combinations(12, 3).len(), 220);
        assert_eq!(combinations(4, 2), vec![vec![0, 1], vec![0, 2], vec![0, 3], vec![1, 2], vec![1, 3], vec![2, 3]]);
    }

    #[test]
    fn the_best_partition_uses_no_player_twice() {
        let groups = vec![(vec![0, 1], 10.0), (vec![0, 2], 9.0), (vec![2, 3], 1.0), (vec![1, 3], 8.5)];
        let chosen = best_partition(&groups, &[0.5, 0.5]).unwrap();
        let mut picked: Vec<usize> = chosen.iter().flat_map(|&i| groups[i].0.clone()).collect();
        picked.sort_unstable();
        assert_eq!(picked, vec![0, 1, 2, 3]);
        assert_eq!(chosen, vec![1, 3], "0-2 with 1-3 (17.5) beats 0-1 with 2-3 (11)");
    }

    #[test]
    fn no_partition_when_there_are_too_few_players() {
        assert!(best_partition(&[(vec![0, 1], 1.0)], &[0.5, 0.5]).is_none());
    }

    #[test]
    fn the_best_unit_takes_the_most_minutes() {
        assert!((slotted(&[40.0, 60.0], &[0.75, 0.25]) - 55.0).abs() < 1e-12);
    }
}
