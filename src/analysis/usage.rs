//! How each skater is used (shift lengths, rest, ice time by score, late in close games,
//! special teams, strength of linemates) and whether goals come late in shifts.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{PlayerRef, TestRow, Verdict, per_60};
use crate::analysis::stints::Stint;
use crate::model::{Game, GameId, PERIOD_SECONDS, PlayerId, Position, Seconds, Strength, period_of};
use crate::stats::describe::{mean, quantile};
use crate::stats::dist::binomial_exact_two_sided;

/// Shifts longer than this are "long" (a typical shift is well under a minute).
const LONG_SHIFT: f64 = 60.0;
/// Rest shorter than this is a quick double shift rather than a real rest.
const MIN_REST: f64 = 5.0;
/// The last stretch of the third period, and the score margin, that count as "late and close".
const LATE_SECONDS: f64 = 300.0;
const CLOSE_MARGIN: i64 = 1;
/// On-ice average time into the shift that splits "fresh" from "tired" for the test.
const TIRED_AFTER: f64 = 40.0;
const MIN_GOALS_FOR_FATIGUE_TEST: u32 = 8;
/// Shift starts are read this far inside a stint, so a player coming on exactly at its start
/// counts as on the ice.
const STINT_PROBE: f64 = 0.05;
/// Goals are placed this long before their recorded time, as stints attribute them.
const GOAL_PROBE: f64 = 0.25;
const AGE_BUCKETS: [(f64, Option<f64>); 5] = [(0.0, Some(15.0)), (15.0, Some(30.0)), (30.0, Some(45.0)), (45.0, Some(60.0)), (60.0, None)];

/// Share of the team's time in one situation that the player was on the ice for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Shares {
    pub leading: Option<f64>,
    pub tied: Option<f64>,
    pub trailing: Option<f64>,
    pub late_and_close: Option<f64>,
    pub power_play: Option<f64>,
    pub penalty_kill: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerUsage {
    pub player: PlayerRef,
    pub games: u32,
    pub shifts: u32,
    pub average_shift: Option<f64>,
    pub median_shift: Option<f64>,
    /// Share of shifts over a minute.
    pub long_shifts: Option<f64>,
    /// Average time between a shift's end and their next shift in the same period.
    pub average_rest: Option<f64>,
    pub shares: Shares,
    /// Ice-time-weighted rating of the teammates on the ice with them at even strength.
    pub linemate_rating: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FatigueBucket {
    pub from_seconds: f64,
    /// `None` for the open-ended last bucket.
    pub to_seconds: Option<f64>,
    pub minutes: f64,
    pub goals_for: u32,
    pub goals_against: u32,
    pub goals_for_per_60: Option<f64>,
    pub goals_against_per_60: Option<f64>,
}

/// Team minutes in each situation, so a share of a short stretch reads as such.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SituationMinutes {
    pub leading: f64,
    pub tied: f64,
    pub trailing: f64,
    pub late_and_close: f64,
    pub power_play: f64,
    pub penalty_kill: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageReport {
    pub players: Vec<PlayerUsage>,
    pub situation_minutes: SituationMinutes,
    /// Even-strength time and goals by how long our skaters had been out, on average.
    pub fatigue: Vec<FatigueBucket>,
    pub tired_after_seconds: f64,
}

/// How far into their current shift each of `players` is at the start of `stint`, averaged.
/// Everyone on the ice ages together, so the average grows by one second per second.
fn average_shift_age_at_start(game: &Game, stint: &Stint) -> Option<f64> {
    let probe = stint.start.0 + STINT_PROBE.min(stint.duration().0 / 2.0);
    let ages: Vec<f64> = stint
        .players
        .iter()
        .filter_map(|id| {
            let shift = game.player(id)?.shifts.iter().find(|s| s.start.0 <= probe && probe <= s.end.0)?;
            Some(stint.start.0 - shift.start.0)
        })
        .collect();
    mean(&ages).map(|a| a.max(0.0))
}

/// Seconds of `[from, to)` (in average time into the shift) that fall in each bucket.
fn bucket_overlaps(from: f64, to: f64) -> [f64; AGE_BUCKETS.len()] {
    let mut overlaps = [0.0; AGE_BUCKETS.len()];
    for (slot, (low, high)) in overlaps.iter_mut().zip(AGE_BUCKETS) {
        let high = high.unwrap_or(f64::INFINITY);
        *slot = (to.min(high) - from.max(low)).max(0.0);
    }
    overlaps
}

fn bucket_of(age: f64) -> usize {
    AGE_BUCKETS
        .iter()
        .position(|(from, to)| age >= *from && to.is_none_or(|to| age < to))
        .unwrap_or(AGE_BUCKETS.len() - 1)
}

/// Our lead during the stint, from the goals before it started.
fn lead_during(game: &Game, stint: &Stint) -> i64 {
    game.goals
        .iter()
        .rev()
        .find(|g| g.time.0 <= stint.start.0)
        .map_or(0, |g| i64::from(g.score_after.0) - i64::from(g.score_after.1))
}

fn is_late_and_close(stint: &Stint, lead: i64) -> bool {
    let third_ends = 3.0 * PERIOD_SECONDS;
    period_of(stint.start) == 3 && stint.start.0 >= third_ends - LATE_SECONDS && lead.abs() <= CLOSE_MARGIN
}

/// Seconds of team time and of each player's time in one situation.
#[derive(Default)]
struct Situation {
    team: f64,
    players: HashMap<PlayerId, f64>,
}

impl Situation {
    fn add(&mut self, stint: &Stint) {
        let seconds = stint.duration().0;
        self.team += seconds;
        for id in &stint.players {
            *self.players.entry(id.clone()).or_default() += seconds;
        }
    }

    fn share(&self, id: &PlayerId) -> Option<f64> {
        (self.team > 0.0).then(|| self.players.get(id).copied().unwrap_or_default() / self.team)
    }
}

struct Situations {
    leading: Situation,
    tied: Situation,
    trailing: Situation,
    late_and_close: Situation,
    power_play: Situation,
    penalty_kill: Situation,
}

fn situations(context: &Context<'_>, games: &HashMap<&GameId, &Game>) -> Situations {
    let mut s = Situations {
        leading: Situation::default(),
        tied: Situation::default(),
        trailing: Situation::default(),
        late_and_close: Situation::default(),
        power_play: Situation::default(),
        penalty_kill: Situation::default(),
    };
    for stint in &context.stints {
        let Some(game) = games.get(&stint.game) else {
            continue;
        };
        match stint.strength {
            Strength::PowerPlay => s.power_play.add(stint),
            Strength::ShortHanded => s.penalty_kill.add(stint),
            Strength::Even => {}
        }
        let lead = lead_during(game, stint);
        match lead.signum() {
            1 => s.leading.add(stint),
            -1 => s.trailing.add(stint),
            _ => s.tied.add(stint),
        }
        if is_late_and_close(stint, lead) {
            s.late_and_close.add(stint);
        }
    }
    s
}

fn shift_lengths(context: &Context<'_>, id: &PlayerId) -> (Vec<f64>, Vec<f64>) {
    let mut lengths = Vec::new();
    let mut rests = Vec::new();
    for game in &context.scope {
        let Some(player) = game.player(id) else {
            continue;
        };
        let mut shifts: Vec<_> = player.shifts.iter().collect();
        shifts.sort_by(|a, b| a.start.0.total_cmp(&b.start.0));
        lengths.extend(shifts.iter().map(|s| s.duration().0));
        for pair in shifts.windows(2) {
            let rest = pair[1].start.0 - pair[0].end.0;
            if period_of(pair[0].start) == period_of(pair[1].start) && rest >= MIN_REST {
                rests.push(rest);
            }
        }
    }
    (lengths, rests)
}

fn linemate_rating(context: &Context<'_>, id: &PlayerId, ratings: &HashMap<PlayerId, f64>) -> Option<f64> {
    let (weighted, weight) = context
        .stints
        .iter()
        .filter(|s| s.strength == Strength::Even && s.has(id))
        .flat_map(|s| s.players.iter().filter(move |p| *p != id).map(move |p| (p, s.duration().0)))
        .filter_map(|(p, seconds)| Some((ratings.get(p)? * seconds, seconds)))
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    (weight > 0.0).then(|| weighted / weight)
}

fn player_usage(context: &Context<'_>, player: &PlayerRef, situations: &Situations, ratings: &HashMap<PlayerId, f64>) -> Option<PlayerUsage> {
    let id = &player.id;
    let (lengths, rests) = shift_lengths(context, id);
    if lengths.is_empty() {
        return None;
    }
    let long = lengths.iter().filter(|l| **l > LONG_SHIFT).count();
    Some(PlayerUsage {
        player: player.clone(),
        games: u32::try_from(context.scope.iter().filter(|g| g.player(id).is_some_and(|p| !p.shifts.is_empty())).count()).ok()?,
        shifts: u32::try_from(lengths.len()).ok()?,
        average_shift: mean(&lengths),
        median_shift: quantile(&lengths, 0.5),
        long_shifts: Some(long as f64 / lengths.len() as f64),
        average_rest: mean(&rests),
        shares: Shares {
            leading: situations.leading.share(id),
            tied: situations.tied.share(id),
            trailing: situations.trailing.share(id),
            late_and_close: situations.late_and_close.share(id),
            power_play: situations.power_play.share(id),
            penalty_kill: situations.penalty_kill.share(id),
        },
        linemate_rating: linemate_rating(context, id, ratings),
    })
}

fn fatigue(context: &Context<'_>, games: &HashMap<&GameId, &Game>) -> Vec<FatigueBucket> {
    let mut buckets: Vec<FatigueBucket> = AGE_BUCKETS
        .iter()
        .map(|(from, to)| FatigueBucket {
            from_seconds: *from,
            to_seconds: *to,
            minutes: 0.0,
            goals_for: 0,
            goals_against: 0,
            goals_for_per_60: None,
            goals_against_per_60: None,
        })
        .collect();
    for stint in context.stints.iter().filter(|s| s.strength == Strength::Even) {
        let Some(game) = games.get(&stint.game) else {
            continue;
        };
        let Some(age) = average_shift_age_at_start(game, stint) else {
            continue;
        };
        for (bucket, seconds) in buckets.iter_mut().zip(bucket_overlaps(age, age + stint.duration().0)) {
            bucket.minutes += seconds / 60.0;
        }
        for goal in &game.goals {
            let at = goal.time.0 - GOAL_PROBE;
            if at < stint.start.0 || at >= stint.end.0 {
                continue;
            }
            let bucket = &mut buckets[bucket_of(age + at - stint.start.0)];
            match goal.scored_by {
                crate::model::Team::Us => bucket.goals_for += 1,
                crate::model::Team::Them => bucket.goals_against += 1,
            }
        }
    }
    for b in &mut buckets {
        b.goals_for_per_60 = per_60(f64::from(b.goals_for), Seconds(b.minutes * 60.0));
        b.goals_against_per_60 = per_60(f64::from(b.goals_against), Seconds(b.minutes * 60.0));
    }
    buckets
}

fn fatigue_test(buckets: &[FatigueBucket], tests: &mut Vec<TestRow>) {
    let family = "Fatigue";
    let (fresh, tired): (Vec<&FatigueBucket>, Vec<&FatigueBucket>) = buckets.iter().partition(|b| b.from_seconds < TIRED_AFTER);
    let sum = |part: &[&FatigueBucket], goals: fn(&FatigueBucket) -> u32| -> (u32, f64) {
        part.iter().fold((0, 0.0), |(g, m), b| (g + goals(b), m + b.minutes))
    };
    for (label, goals) in [("against", (|b: &FatigueBucket| b.goals_against) as fn(&FatigueBucket) -> u32), ("for", |b: &FatigueBucket| b.goals_for)] {
        let (tired_goals, tired_minutes) = sum(&tired, goals);
        let (fresh_goals, fresh_minutes) = sum(&fresh, goals);
        let question = format!("Do we give up or score more goals ({label}) when our skaters are deep into their shifts?");
        let method = format!("Exact conditional Poisson test: even-strength goals {label} when our skaters had been out {TIRED_AFTER:.0}+ seconds on average vs less");
        let n = format!("{} even-strength goals {label}", tired_goals + fresh_goals);
        let total = tired_goals + fresh_goals;
        if total < MIN_GOALS_FOR_FATIGUE_TEST || tired_minutes <= 0.0 || fresh_minutes <= 0.0 {
            tests.push(TestRow::not_enough(family, &question, &method, n, "needs 8+ even-strength goals"));
            continue;
        }
        let p = binomial_exact_two_sided(u64::from(tired_goals), u64::from(total), tired_minutes / (tired_minutes + fresh_minutes));
        let ratio = (fresh_goals > 0).then(|| (f64::from(tired_goals) / tired_minutes) / (f64::from(fresh_goals) / fresh_minutes));
        tests.push(TestRow {
            family: family.to_owned(),
            question,
            method,
            statistic_label: "Goals late in shifts".to_owned(),
            statistic: Some(f64::from(tired_goals)),
            df: None,
            p: Some(p),
            p_adjusted: None,
            effect_label: "Rate ratio (late in shifts ÷ early)".to_owned(),
            effect: ratio,
            ci: None,
            n,
            secondary: None,
            verdict: Verdict::from_adjusted_p(Some(p)),
            plain: ratio.map_or_else(
                || "No goals early in shifts to compare with.".to_owned(),
                |r| format!("Goals {label} come {r:.1}× as fast late in shifts as early."),
            ),
            assumptions: "Goals arrive at a steady rate within each group; on-ice time into the shift is measured at the middle of each stretch of play.".to_owned(),
        });
    }
}

#[must_use]
pub(crate) fn usage(context: &Context<'_>, ratings: &HashMap<PlayerId, f64>, tests: &mut Vec<TestRow>) -> UsageReport {
    let games: HashMap<&GameId, &Game> = context.scope.iter().map(|g| (&g.id, *g)).collect();
    let situations = situations(context, &games);
    let mut skaters: Vec<&PlayerRef> = context
        .roster
        .values()
        .filter(|p| matches!(p.position, Position::Forward | Position::Defence))
        .collect();
    skaters.sort_by(|a, b| a.id.cmp(&b.id));
    let mut players: Vec<PlayerUsage> = skaters
        .into_iter()
        .filter_map(|p| player_usage(context, p, &situations, ratings))
        .collect();
    players.sort_by(|a, b| b.shifts.cmp(&a.shifts).then_with(|| a.player.id.cmp(&b.player.id)));
    let fatigue = fatigue(context, &games);
    fatigue_test(&fatigue, tests);
    UsageReport {
        situation_minutes: SituationMinutes {
            leading: situations.leading.team / 60.0,
            tied: situations.tied.team / 60.0,
            trailing: situations.trailing.team / 60.0,
            late_and_close: situations.late_and_close.team / 60.0,
            power_play: situations.power_play.team / 60.0,
            penalty_kill: situations.penalty_kill.team / 60.0,
        },
        players,
        fatigue,
        tired_after_seconds: TIRED_AFTER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stint(start: f64, end: f64, players: &[&str]) -> Stint {
        Stint {
            game: GameId("g".into()),
            start: Seconds(start),
            end: Seconds(end),
            players: players.iter().map(|p| PlayerId((*p).into())).collect(),
            strength: Strength::Even,
            goals_for: 0,
            goals_against: 0,
        }
    }

    #[test]
    fn a_stretch_of_play_is_split_across_the_buckets_it_passes_through() {
        let overlaps = bucket_overlaps(10.0, 50.0);
        for (got, want) in overlaps.iter().zip([5.0, 15.0, 15.0, 5.0, 0.0]) {
            assert!((got - want).abs() < 1e-12, "{overlaps:?}");
        }
        assert!((bucket_overlaps(70.0, 100.0)[4] - 30.0).abs() < 1e-12);
    }

    #[test]
    fn shift_age_buckets_cover_every_age() {
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(44.9), 2);
        assert_eq!(bucket_of(45.0), 3);
        assert_eq!(bucket_of(300.0), AGE_BUCKETS.len() - 1);
    }

    #[test]
    fn a_player_on_for_half_the_situation_gets_half_the_share() {
        let mut s = Situation::default();
        s.add(&stint(0.0, 60.0, &["A", "B"]));
        s.add(&stint(60.0, 120.0, &["B"]));
        assert!((s.share(&PlayerId("A".into())).unwrap() - 0.5).abs() < 1e-12);
        assert!((s.share(&PlayerId("B".into())).unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(s.share(&PlayerId("C".into())), Some(0.0));
    }

    #[test]
    fn late_and_close_is_the_last_five_minutes_of_the_third_within_a_goal() {
        assert!(is_late_and_close(&stint(3400.0, 3450.0, &[]), 1));
        assert!(!is_late_and_close(&stint(3400.0, 3450.0, &[]), 2));
        assert!(!is_late_and_close(&stint(3200.0, 3250.0, &[]), 0));
    }
}
