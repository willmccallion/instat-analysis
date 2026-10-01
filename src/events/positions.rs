//! Forward or defence for skaters the export doesn't label (it never says).
//!
//! With five skaters out at even strength a team has two defencemen on the ice, so the D are
//! the group for which that holds most of the time. Wingers also come two to a line, so the
//! search starts from the players who play deepest within each zone and moves one player at
//! a time from there.

use std::collections::{BTreeSet, HashMap};

use super::{RINK_LENGTH_M, Side, on_shift, strength_at};
use crate::model::{Position, Seconds, SkaterPosition, Strength};

/// Faceoffs in a game that mark a skater as a centre, never a defenceman.
const CENTRE_FACEOFFS: usize = 3;
/// Blue line distance from the end boards (75 ft).
const BLUE_LINE_M: f64 = 22.86;
/// Uncontested actions, whose spots are in the acting team's frame.
const PLACED_ACTIONS: [&str; 8] = ["Passes", "Puck recoveries", "Puck losses", "Shots", "Dump outs", "Breakouts", "Entries", "Dump ins"];
const SKATERS_AT_EVEN_STRENGTH: usize = 5;
const DEFENCE_ON_ICE: usize = 2;
const FEWEST_DEFENCE: usize = 4;
const MOST_DEFENCE: usize = 8;
/// Share of five-skater time a change must add to be taken, so noise doesn't move players.
const MIN_GAIN: f64 = 0.01;

/// A stretch with the same five skaters out at even strength.
struct Stretch {
    on_ice: Vec<usize>,
    seconds: f64,
}

/// Every skater's position: `fixed` ones as given, the rest worked out from `side`.
pub(super) fn positions(side: &Side<'_>, skaters: &[&str], fixed: &HashMap<&str, SkaterPosition>) -> HashMap<String, Position> {
    let centres = centres(side, skaters);
    let depths = zone_depths(side, skaters);
    let mut movable: Vec<usize> = (0..skaters.len()).filter(|&i| !fixed.contains_key(skaters[i]) && !centres.contains(&i)).collect();
    movable.sort_by(|&a, &b| depths[a].total_cmp(&depths[b]));
    let fixed_defence: Vec<usize> = (0..skaters.len()).filter(|&i| fixed.get(skaters[i]) == Some(&SkaterPosition::Defence)).collect();
    let defence = choose_defence(skaters.len(), &fixed_defence, &movable, &stretches(side, skaters));
    skaters
        .iter()
        .enumerate()
        .map(|(i, name)| ((*name).to_owned(), if defence[i] { Position::Defence } else { Position::Forward }))
        .collect()
}

fn centres(side: &Side<'_>, skaters: &[&str]) -> BTreeSet<usize> {
    let faceoffs = |name: &str| side.team.actions.iter().filter(|a| a.name == "Faceoffs" && a.player.as_deref() == Some(name)).count();
    (0..skaters.len()).filter(|&i| faceoffs(skaters[i]) >= CENTRE_FACEOFFS).collect()
}

fn mean(values: &[f64]) -> Option<f64> {
    let n = u32::try_from(values.len()).ok().filter(|&n| n > 0)?;
    Some(values.iter().sum::<f64>() / f64::from(n))
}

/// How deep each skater plays within the offensive and defensive zones, lower is deeper: a
/// defenceman holds the point in one and stays near the net in the other. A zone a skater
/// has no actions in counts as the team's average there.
fn zone_depths(side: &Side<'_>, skaters: &[&str]) -> Vec<f64> {
    let spots = |name: Option<&str>, in_zone: fn(f64) -> bool| -> Vec<f64> {
        side.team
            .actions
            .iter()
            .filter(|a| PLACED_ACTIONS.contains(&a.name.as_str()) && name.is_none_or(|n| a.player.as_deref() == Some(n)))
            .filter_map(|a| a.position.map(|(x, _)| x))
            .filter(|x| in_zone(*x))
            .collect()
    };
    let offensive: fn(f64) -> bool = |x| x > RINK_LENGTH_M - BLUE_LINE_M;
    let defensive: fn(f64) -> bool = |x| x < BLUE_LINE_M;
    let team_offensive = mean(&spots(None, offensive)).unwrap_or_default();
    let team_defensive = mean(&spots(None, defensive)).unwrap_or_default();
    skaters
        .iter()
        .map(|name| mean(&spots(Some(name), offensive)).unwrap_or(team_offensive) + mean(&spots(Some(name), defensive)).unwrap_or(team_defensive))
        .collect()
}

fn stretches(side: &Side<'_>, skaters: &[&str]) -> Vec<Stretch> {
    let mut cuts: Vec<f64> = skaters.iter().filter_map(|n| side.shifts.get(*n)).flatten().flat_map(|s| [s.start.0, s.end.0]).collect();
    cuts.extend(side.advantages.iter().flat_map(|(i, _)| [i.start.0, i.end.0]));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|w| {
            let middle = Seconds(f64::midpoint(w[0], w[1]));
            if strength_at(&side.advantages, middle) != Strength::Even {
                return None;
            }
            let on_ice: Vec<usize> = (0..skaters.len()).filter(|&i| side.shifts.get(skaters[i]).is_some_and(|s| on_shift(s, middle))).collect();
            (on_ice.len() == SKATERS_AT_EVEN_STRENGTH).then_some(Stretch { on_ice, seconds: w[1] - w[0] })
        })
        .collect()
}

/// Share of five-skater time with exactly two of `defence` on the ice.
fn two_defence_share(stretches: &[Stretch], defence: &[bool]) -> f64 {
    let total: f64 = stretches.iter().map(|s| s.seconds).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let paired: f64 = stretches.iter().filter(|s| s.on_ice.iter().filter(|&&i| defence[i]).count() == DEFENCE_ON_ICE).map(|s| s.seconds).sum();
    paired / total
}

/// Membership of the defence among `players` skaters: `fixed` always, plus those of `movable`
/// (deepest first) that best explain two defencemen on the ice, climbing from the deepest
/// 4 to 8. Without five-skater time to go on, the deepest third of the roster.
fn choose_defence(players: usize, fixed: &[usize], movable: &[usize], stretches: &[Stretch]) -> Vec<bool> {
    let with = |extra: &[usize]| -> Vec<bool> {
        let mut defence = vec![false; players];
        for &i in fixed.iter().chain(extra) {
            defence[i] = true;
        }
        defence
    };
    if stretches.is_empty() {
        let wanted = ((players + 1) / 3).saturating_sub(fixed.len()).min(movable.len());
        return with(&movable[..wanted]);
    }
    let most = MOST_DEFENCE.saturating_sub(fixed.len()).min(movable.len());
    let fewest = FEWEST_DEFENCE.saturating_sub(fixed.len()).min(most);
    (fewest..=most)
        .map(|k| climb(with(&movable[..k]), movable, stretches))
        .reduce(|best, next| if next.0 > best.0 + f64::EPSILON { next } else { best })
        .map_or_else(|| with(&[]), |(_, defence)| defence)
}

/// From `defence`, one-player changes while each adds enough; the share reached and the defence.
fn climb(start: Vec<bool>, movable: &[usize], stretches: &[Stretch]) -> (f64, Vec<bool>) {
    let mut share = two_defence_share(stretches, &start);
    let mut defence = start;
    loop {
        let size = defence.iter().filter(|d| **d).count();
        let best_move = one_player_moves(&defence, movable, size)
            .map(|d| (two_defence_share(stretches, &d), d))
            .reduce(|best, next| if next.0 > best.0 + f64::EPSILON { next } else { best });
        match best_move {
            Some((next_share, next)) if next_share > share + MIN_GAIN => (share, defence) = (next_share, next),
            _ => return (share, defence),
        }
    }
}

/// Every defence one movable player away from `defence` that keeps a plausible size.
fn one_player_moves<'a>(defence: &'a [bool], movable: &'a [usize], size: usize) -> impl Iterator<Item = Vec<bool>> + 'a {
    let toggled = move |changes: &[usize]| {
        let mut next = defence.to_vec();
        for &i in changes {
            next[i] = !next[i];
        }
        next
    };
    let (members, others): (Vec<usize>, Vec<usize>) = movable.iter().partition(|&&i| defence[i]);
    let removals = members.clone().into_iter().filter(move |_| size > FEWEST_DEFENCE).map(move |m| toggled(&[m]));
    let additions = others.clone().into_iter().filter(move |_| size < MOST_DEFENCE).map(move |o| toggled(&[o]));
    let swaps = members.into_iter().flat_map(move |m| others.clone().into_iter().map(move |o| toggled(&[m, o])));
    removals.chain(additions).chain(swaps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five D rotating in pairs behind four lines whose wingers also come in twos.
    fn rotation(defence: &[usize], lines: &[[usize; 3]]) -> Vec<Stretch> {
        let mut stretches = Vec::new();
        for shift in 0..60 {
            let (a, b) = (defence[shift % defence.len()], defence[(shift + 1) % defence.len()]);
            let line = lines[shift % lines.len()];
            stretches.push(Stretch { on_ice: vec![a, b, line[0], line[1], line[2]], seconds: 40.0 });
        }
        stretches
    }

    fn members(defence: &[bool]) -> Vec<usize> {
        (0..defence.len()).filter(|&i| defence[i]).collect()
    }

    const LINES: [[usize; 3]; 4] = [[5, 6, 7], [8, 9, 10], [11, 12, 13], [14, 15, 16]];

    #[test]
    fn five_defencemen_are_found_when_the_depth_order_starts_right() {
        let stretches = rotation(&[0, 1, 2, 3, 4], &LINES);
        let centres = [6, 9, 12, 15];
        let movable: Vec<usize> = (0..17).filter(|i| !centres.contains(i)).collect();

        let defence = choose_defence(17, &[], &movable, &stretches);

        assert_eq!(members(&defence), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_defenceman_ranked_shallow_is_swapped_back_in() {
        let stretches = rotation(&[0, 1, 2, 3, 4, 5], &[[6, 7, 8], [9, 10, 11], [12, 13, 14], [15, 16, 17]]);
        let movable = vec![0, 1, 2, 3, 4, 6, 8, 5, 9, 11, 12, 14, 15, 17];

        let defence = choose_defence(18, &[], &movable, &stretches);

        assert_eq!(members(&defence), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_coach_set_defenceman_stays_and_counts_toward_the_pairs() {
        let stretches = rotation(&[0, 1, 2, 3, 4], &LINES);
        let movable: Vec<usize> = (0..17).filter(|&i| i != 3).collect();

        let defence = choose_defence(17, &[3], &movable, &stretches);

        assert_eq!(members(&defence), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn without_five_skater_time_the_deepest_third_play_defence() {
        let defence = choose_defence(6, &[], &[4, 2, 0, 1, 3, 5], &[]);

        assert_eq!(members(&defence), vec![2, 4]);
    }
}
