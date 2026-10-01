//! Line builder: forward lines and defence pairs ranked by the individual impact ratings.
//!
//! For each way of judging a lineup, every possible unit is ranked and the best full lineup
//! that uses each player at most once is found.
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

/// What a lineup is chosen to maximise. The first is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    /// Even-strength goals for minus against per 60.
    GoalDifferential,
    GoalsFor,
    /// Fewest goals against.
    GoalsAgainst,
    /// Share of shot attempts.
    ShotShare,
}

const OBJECTIVES: [Objective; 4] = [Objective::GoalDifferential, Objective::GoalsFor, Objective::GoalsAgainst, Objective::ShotShare];

impl Objective {
    const fn uses_goals(self) -> bool {
        !matches!(self, Self::ShotShare)
    }

    /// Higher is better.
    fn score(self, prediction: &Prediction) -> Option<f64> {
        match self {
            Self::GoalDifferential => Some(prediction.goals_for_per_60? - prediction.goals_against_per_60?),
            Self::GoalsFor => prediction.goals_for_per_60,
            Self::GoalsAgainst => prediction.goals_against_per_60.map(|a| -a),
            Self::ShotShare => prediction.shot_share_pct,
        }
    }
}

/// What the ratings predict for a unit with average teammates around it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Prediction {
    pub goals_for_per_60: Option<f64>,
    pub goals_against_per_60: Option<f64>,
    /// Shot-attempt share, percent.
    pub shot_share_pct: Option<f64>,
}

impl Prediction {
    fn of(models: &Models<'_>, players: &[&PlayerId]) -> Self {
        let goals = models.goals.and_then(|r| r.expected_rates(players));
        Self {
            goals_for_per_60: goals.map(|g| g.for_per_60),
            goals_against_per_60: goals.map(|g| g.against_per_60),
            shot_share_pct: models.shots.and_then(|r| r.expected_share(players)),
        }
    }

    /// Each field weighted by `shares`; absent when any unit lacks it.
    fn weighted(predictions: &[Self], shares: &[f64]) -> Self {
        let combine = |field: fn(&Self) -> Option<f64>| -> Option<f64> {
            predictions.iter().zip(shares).map(|(p, w)| field(p).map(|v| v * w)).sum()
        };
        Self {
            goals_for_per_60: combine(|p| p.goals_for_per_60),
            goals_against_per_60: combine(|p| p.goals_against_per_60),
            shot_share_pct: combine(|p| p.shot_share_pct),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Centre,
    Wing,
    Defence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub player: PlayerRef,
    pub role: Role,
}

/// How an exact group did in the games in scope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Observed {
    pub minutes: f64,
    pub shot_share_pct: Option<f64>,
    pub goals_for: u32,
    pub goals_against: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Combo {
    /// For a forward line with a faceoff taker, wing, centre, wing.
    pub slots: Vec<Slot>,
    pub predicted: Prediction,
    pub observed: Option<Observed>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lineup {
    /// Best first; the best plays the most.
    pub units: Vec<Combo>,
    /// Share of the minutes each unit gets, most first: the split the current most-used units
    /// get, or equal when too few are known.
    pub minute_shares: Vec<f64>,
    pub predicted: Prediction,
    /// The current most-used units under the same model and minutes.
    pub current: Option<Prediction>,
    /// Rated players left out of the lineup.
    pub extras: Vec<PlayerRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineGroup {
    pub best: Vec<Combo>,
    pub lineup: Option<Lineup>,
    /// Units a full lineup has; fewer when fewer players are rated.
    pub full_units: usize,
    /// Whether every forward line was required to include a faceoff taker.
    pub needs_centre: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectiveLineups {
    pub objective: Objective,
    pub forwards: Option<LineGroup>,
    pub defence: Option<LineGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineupReport {
    pub games: usize,
    /// One per objective, the default first.
    pub choices: Vec<ObjectiveLineups>,
}

/// The ratings behind one position's predictions.
#[derive(Clone, Copy)]
struct Models<'a> {
    goals: Option<&'a Ratings>,
    shots: Option<&'a Ratings>,
}

impl Models<'_> {
    /// The ratings that rank players under `objective`.
    const fn deciding(&self, objective: Objective) -> Option<&Ratings> {
        if objective.uses_goals() { self.goals } else { self.shots }
    }
}

/// How one position is built into units.
struct Shape<'a> {
    position: Position,
    size: usize,
    units: usize,
    rows: &'a [UnitRow],
    models: Models<'a>,
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

/// Depth-first search over disjoint groups taken best first, so the k-th group chosen gets the
/// k-th largest share, pruned once even the best remaining groups cannot beat the best found.
struct PartitionSearch<'a> {
    groups: &'a [(Vec<usize>, f64)],
    order: Vec<usize>,
    shares: &'a [f64],
    /// `share_left[k]` is the sum of `shares[k..]`.
    share_left: Vec<f64>,
    used: Vec<bool>,
    chosen: Vec<usize>,
    best: Option<(f64, Vec<usize>)>,
}

impl PartitionSearch<'_> {
    fn visit(&mut self, from: usize, score: f64) {
        let slot = self.chosen.len();
        if slot == self.shares.len() {
            if self.best.as_ref().is_none_or(|(s, _)| score > *s + 1e-12) {
                self.best = Some((score, self.chosen.clone()));
            }
            return;
        }
        for position in from..self.order.len() {
            let index = self.order[position];
            let (members, value) = &self.groups[index];
            if self.best.as_ref().is_some_and(|(s, _)| score + value * self.share_left[slot] <= *s + 1e-12) {
                return;
            }
            if members.iter().any(|&m| self.used[m]) {
                continue;
            }
            for &m in members {
                self.used[m] = true;
            }
            self.chosen.push(index);
            self.visit(position + 1, score + value * self.shares[slot]);
            self.chosen.pop();
            for &m in members {
                self.used[m] = false;
            }
        }
    }
}

/// Indexes of the `shares.len()` disjoint groups, each given as (members, value), with the
/// largest minute-weighted value when the best group plays the most. `shares` is most first.
fn best_partition(groups: &[(Vec<usize>, f64)], shares: &[f64]) -> Option<Vec<usize>> {
    let players = groups.iter().flat_map(|(m, _)| m.iter().copied()).max()? + 1;
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| groups[b].1.total_cmp(&groups[a].1));
    let mut share_left: Vec<f64> = shares.iter().rev().scan(0.0, |sum, s| { *sum += s; Some(*sum) }).collect();
    share_left.reverse();
    let mut search = PartitionSearch { groups, order, shares, share_left, used: vec![false; players], chosen: Vec::new(), best: None };
    search.visit(0, 0.0);
    search.best.map(|(_, chosen)| chosen)
}

fn sorted_ids(players: &[PlayerRef]) -> Vec<PlayerId> {
    let mut ids: Vec<PlayerId> = players.iter().map(|p| p.id.clone()).collect();
    ids.sort();
    ids
}

/// Members in card order with their roles: a forward line's busiest faceoff taker in the middle.
fn slots(members: &[PlayerRef], position: Position, centres: &HashMap<PlayerId, f64>) -> Vec<Slot> {
    if position != Position::Forward {
        return members.iter().map(|p| Slot { player: p.clone(), role: Role::Defence }).collect();
    }
    let centre = members
        .iter()
        .enumerate()
        .filter_map(|(i, p)| centres.get(&p.id).map(|rate| (i, *rate)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i);
    let wings = members.iter().enumerate().filter(|(i, _)| Some(*i) != centre).map(|(_, p)| Slot { player: p.clone(), role: Role::Wing });
    let mut ordered: Vec<Slot> = wings.collect();
    if let Some(i) = centre {
        ordered.insert(ordered.len().min(1), Slot { player: members[i].clone(), role: Role::Centre });
    }
    ordered
}

fn combo(shape: &Shape<'_>, members: &[PlayerRef], observed: &HashMap<Vec<PlayerId>, &UnitRow>, centres: &HashMap<PlayerId, f64>) -> Combo {
    let ids: Vec<&PlayerId> = members.iter().map(|p| &p.id).collect();
    Combo {
        slots: slots(members, shape.position, centres),
        predicted: Prediction::of(&shape.models, &ids),
        observed: observed.get(&sorted_ids(members)).map(|r| Observed {
            minutes: r.toi.minutes(),
            shot_share_pct: r.corsi_pct,
            goals_for: r.goals_for,
            goals_against: r.goals_against,
        }),
    }
}

fn eligible_players(context: &Context<'_>, ratings: &Ratings, position: Position) -> Vec<PlayerRef> {
    let mut players: Vec<PlayerRef> = ratings
        .offence
        .keys()
        .filter_map(|id| context.roster.get(id))
        .filter(|p| p.position == position)
        .cloned()
        .collect();
    players.sort_by(|a, b| a.id.cmp(&b.id));
    players
}

fn group(context: &Context<'_>, shape: &Shape<'_>, objective: Objective, centres: &HashMap<PlayerId, f64>) -> Option<LineGroup> {
    let players = eligible_players(context, shape.models.deciding(objective)?, shape.position);
    if players.len() < shape.size {
        return None;
    }
    let units = (players.len() / shape.size).min(shape.units);
    let observed: HashMap<Vec<PlayerId>, &UnitRow> = shape.rows.iter().map(|r| (sorted_ids(&r.players), r)).collect();
    let needs_centre = shape.size == 3 && players.iter().filter(|p| centres.contains_key(&p.id)).count() >= units;
    let scored: Vec<(Vec<usize>, Combo, f64)> = combinations(players.len(), shape.size)
        .into_iter()
        .filter_map(|members| {
            let refs: Vec<PlayerRef> = members.iter().map(|&i| players[i].clone()).collect();
            let combo = combo(shape, &refs, &observed, centres);
            let score = objective.score(&combo.predicted)?;
            Some((members, combo, score))
        })
        .collect();
    let mut best: Vec<&(Vec<usize>, Combo, f64)> = scored.iter().collect();
    best.sort_by(|a, b| b.2.total_cmp(&a.2));
    let candidates: Vec<&(Vec<usize>, Combo, f64)> = scored
        .iter()
        .filter(|(members, _, _)| !needs_centre || members.iter().any(|&i| centres.contains_key(&players[i].id)))
        .collect();
    let groups: Vec<(Vec<usize>, f64)> = candidates.iter().map(|(members, _, score)| (members.clone(), *score)).collect();
    let (minute_shares, current) = current_usage(&shape.models, shape.rows, units);
    let lineup = best_partition(&groups, &minute_shares).map(|chosen| {
        let mut picked: Vec<&(Vec<usize>, Combo, f64)> = chosen.iter().map(|&i| candidates[i]).collect();
        picked.sort_by(|a, b| b.2.total_cmp(&a.2));
        let in_lineup: Vec<usize> = picked.iter().flat_map(|(members, _, _)| members.iter().copied()).collect();
        let units: Vec<Combo> = picked.into_iter().map(|(_, c, _)| c.clone()).collect();
        let predictions: Vec<Prediction> = units.iter().map(|c| c.predicted).collect();
        Lineup {
            predicted: Prediction::weighted(&predictions, &minute_shares),
            units,
            current,
            extras: (0..players.len()).filter(|i| !in_lineup.contains(i)).map(|i| players[i].clone()).collect(),
            minute_shares: minute_shares.clone(),
        }
    });
    Some(LineGroup {
        best: best.into_iter().take(SHOWN_COMBOS).map(|(_, c, _)| c.clone()).collect(),
        lineup,
        full_units: shape.units,
        needs_centre,
    })
}

/// The minute split of the `units` most-used current units (equal when fewer are known), most
/// first, and what the ratings predict for those units with those minutes.
fn current_usage(models: &Models<'_>, rows: &[UnitRow], units: usize) -> (Vec<f64>, Option<Prediction>) {
    let mut used: Vec<&UnitRow> = rows.iter().filter(|r| r.toi.0 > 0.0).collect();
    used.sort_by(|a, b| b.toi.0.total_cmp(&a.toi.0));
    used.truncate(units);
    let minutes: f64 = used.iter().map(|r| r.toi.0).sum();
    if used.len() < units || minutes <= 0.0 {
        return (vec![1.0 / units as f64; units], None);
    }
    let shares: Vec<f64> = used.iter().map(|r| r.toi.0 / minutes).collect();
    let predictions: Vec<Prediction> = used
        .iter()
        .map(|r| {
            let ids: Vec<&PlayerId> = r.players.iter().map(|p| &p.id).collect();
            Prediction::of(models, &ids)
        })
        .collect();
    let current = Prediction::weighted(&predictions, &shares);
    (shares, Some(current))
}

/// Faceoffs per game of every player who takes enough to play centre.
fn centres(context: &Context<'_>) -> HashMap<PlayerId, f64> {
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
        .map(|(id, (taken, games))| (id.clone(), f64::from(taken) / f64::from(games.max(1))))
        .filter(|(_, per_game)| *per_game >= CENTRE_FACEOFFS_PER_GAME)
        .collect()
}

#[must_use]
pub(crate) fn lineup(context: &Context<'_>, ratings: &HashMap<UnitKind, Ratings>, goal_ratings: Option<&Ratings>, units: &UnitsReport) -> LineupReport {
    let centres = centres(context);
    let forwards = Shape {
        position: Position::Forward,
        size: 3,
        units: FORWARD_LINES,
        rows: &units.forward_lines,
        models: Models { goals: goal_ratings, shots: ratings.get(&UnitKind::ForwardLine) },
    };
    let defence = Shape {
        position: Position::Defence,
        size: 2,
        units: DEFENCE_PAIRS,
        rows: &units.defence_pairs,
        models: Models { goals: goal_ratings, shots: ratings.get(&UnitKind::DefencePair) },
    };
    LineupReport {
        games: context.scope.len(),
        choices: OBJECTIVES
            .iter()
            .map(|&objective| ObjectiveLineups {
                objective,
                forwards: group(context, &forwards, objective, &centres),
                defence: group(context, &defence, objective, &centres),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Best score over every set of `shares.len()` disjoint groups, with the best playing most.
    fn exhaustive_score(groups: &[(Vec<usize>, f64)], shares: &[f64]) -> Option<f64> {
        fn extend(groups: &[(Vec<usize>, f64)], shares: &[f64], from: usize, used: &mut Vec<bool>, values: &mut Vec<f64>, best: &mut Option<f64>) {
            if values.len() == shares.len() {
                let score = score_of_values(values, shares);
                *best = Some(best.map_or(score, |b| b.max(score)));
                return;
            }
            for (index, (members, value)) in groups.iter().enumerate().skip(from) {
                if members.iter().any(|&m| used[m]) {
                    continue;
                }
                for &m in members {
                    used[m] = true;
                }
                values.push(*value);
                extend(groups, shares, index + 1, used, values, best);
                values.pop();
                for &m in members {
                    used[m] = false;
                }
            }
        }
        let players = groups.iter().flat_map(|(m, _)| m.iter().copied()).max()? + 1;
        let mut best = None;
        extend(groups, shares, 0, &mut vec![false; players], &mut Vec::new(), &mut best);
        best
    }

    fn score_of_values(values: &[f64], shares: &[f64]) -> f64 {
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| b.total_cmp(a));
        sorted.iter().zip(shares).map(|(v, w)| v * w).sum()
    }

    fn score_of(groups: &[(Vec<usize>, f64)], shares: &[f64], chosen: &[usize]) -> f64 {
        let values: Vec<f64> = chosen.iter().map(|&i| groups[i].1).collect();
        score_of_values(&values, shares)
    }

    /// Triples valued like the ratings value lines: the members' sum plus a little noise.
    fn additive_triples(players: usize, rng: &mut crate::stats::random::Rng) -> Vec<(Vec<usize>, f64)> {
        let ratings: Vec<f64> = (0..players).map(|_| crate::stats::random::standard_normal(rng)).collect();
        combinations(players, 3)
            .into_iter()
            .map(|m| {
                let value = m.iter().map(|&i| ratings[i]).sum::<f64>() + 0.1 * crate::stats::random::standard_normal(rng);
                (m, value)
            })
            .collect()
    }

    fn random_triples(players: usize, rng: &mut crate::stats::random::Rng) -> Vec<(Vec<usize>, f64)> {
        combinations(players, 3).into_iter().map(|m| (m, crate::stats::random::standard_normal(rng))).collect()
    }

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
        let mut chosen = chosen;
        chosen.sort_unstable();
        assert_eq!(chosen, vec![1, 3], "0-2 with 1-3 (17.5) beats 0-1 with 2-3 (11)");
    }

    #[test]
    fn no_partition_when_there_are_too_few_players() {
        assert!(best_partition(&[(vec![0, 1], 1.0)], &[0.5, 0.5]).is_none());
    }

    #[test]
    fn the_pruned_search_matches_exhaustive_search() {
        let mut rng = crate::stats::random::seeded(11);
        for (players, shares) in [(9, vec![0.4, 0.35, 0.25]), (11, vec![0.4, 0.35, 0.25]), (12, vec![0.3, 0.27, 0.23, 0.2])] {
            for trial in 0..10 {
                let groups = if trial % 2 == 0 { random_triples(players, &mut rng) } else { additive_triples(players, &mut rng) };
                let chosen = best_partition(&groups, &shares).unwrap();
                let expected = exhaustive_score(&groups, &shares).unwrap();
                assert!((score_of(&groups, &shares, &chosen) - expected).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn the_search_handles_negative_values() {
        let groups = vec![(vec![0, 1], -1.0), (vec![2, 3], -3.0), (vec![0, 2], -0.5), (vec![1, 3], -0.6)];
        let mut chosen = best_partition(&groups, &[0.6, 0.4]).unwrap();
        chosen.sort_unstable();
        assert_eq!(chosen, vec![2, 3]);
    }

    #[test]
    fn a_large_roster_finishes_quickly() {
        let mut rng = crate::stats::random::seeded(3);
        let groups = additive_triples(24, &mut rng);
        let started = std::time::Instant::now();
        assert!(best_partition(&groups, &[0.3, 0.27, 0.23, 0.2]).is_some());
        assert!(started.elapsed().as_secs_f64() < 0.5, "took {:?}", started.elapsed());
    }

    #[test]
    fn a_line_puts_its_busiest_faceoff_taker_at_centre() {
        let player = |id: &str| PlayerRef { id: PlayerId(id.to_owned()), name: id.to_owned(), jersey: None, position: Position::Forward };
        let centres: HashMap<PlayerId, f64> = [(PlayerId("b".to_owned()), 4.0), (PlayerId("c".to_owned()), 9.0)].into_iter().collect();
        let line = slots(&[player("a"), player("b"), player("c")], Position::Forward, &centres);
        let order: Vec<(&str, Role)> = line.iter().map(|s| (s.player.name.as_str(), s.role)).collect();
        assert_eq!(order, vec![("a", Role::Wing), ("c", Role::Centre), ("b", Role::Wing)]);
    }

    #[test]
    fn goals_against_ranks_the_stingiest_unit_highest() {
        let stingy = Prediction { goals_for_per_60: Some(1.0), goals_against_per_60: Some(0.5), shot_share_pct: None };
        let leaky = Prediction { goals_for_per_60: Some(3.0), goals_against_per_60: Some(2.0), shot_share_pct: None };
        let score = |p| Objective::GoalsAgainst.score(&p).unwrap();
        assert!(score(stingy) > score(leaky));
        assert!(Objective::GoalDifferential.score(&leaky).unwrap() > Objective::GoalDifferential.score(&stingy).unwrap());
    }
}
