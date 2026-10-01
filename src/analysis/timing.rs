//! When goals happen: by stretch of the game, right after a goal (momentum), and how games
//! end from each lead after a period, after scoring or conceding first, and from behind.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{TestRow, Verdict};
use crate::analysis::team::{Outcome, outcome};
use crate::model::{Game, Goal, PERIOD_SECONDS, Seconds, Team};
use crate::stats::dist::binomial_exact_two_sided;

const SEGMENT_SECONDS: f64 = 300.0;
const REGULATION_SEGMENTS: usize = 12;
/// How long after a goal counts as "right after" for momentum.
pub const RESPONSE_WINDOW: f64 = 120.0;
/// Fewer goals than this in total and the momentum question can't be answered.
const MIN_GOALS_FOR_RESPONSE: u32 = 8;

/// Goals in one five-minute stretch of regulation, or in overtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub start_minute: u32,
    /// `None` for overtime.
    pub end_minute: Option<u32>,
    pub goals_for: u32,
    pub goals_against: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Record {
    pub wins: u32,
    pub losses: u32,
    pub overtime_losses: u32,
    pub ties: u32,
}

impl Record {
    const fn add(&mut self, result: Outcome) {
        match result {
            Outcome::Win => self.wins += 1,
            Outcome::Loss => self.losses += 1,
            Outcome::OvertimeLoss => self.overtime_losses += 1,
            Outcome::Tie => self.ties += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Lead {
    Leading,
    Tied,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LeadRecord {
    pub after_period: u32,
    pub lead: Lead,
    pub record: Record,
}

/// Whose goals follow a goal, and how quickly, against the usual rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Response {
    /// The goal that starts each window.
    pub after: Team,
    /// The goals counted inside the windows.
    pub scorer: Team,
    pub windows: u32,
    pub goals_in_windows: u32,
    pub minutes_in_windows: f64,
    pub goals_elsewhere: u32,
    pub minutes_elsewhere: f64,
    /// Goals per 60 inside the windows divided by goals per 60 elsewhere.
    pub rate_ratio: Option<f64>,
    pub p: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimingReport {
    pub segments: Vec<Segment>,
    pub responses: Vec<Response>,
    pub response_window_minutes: f64,
    pub scored_first: Record,
    pub conceded_first: Record,
    pub scoreless: Record,
    pub by_lead: Vec<LeadRecord>,
    /// Games we trailed at some point and still took points from.
    pub comebacks: u32,
    pub games_trailed: u32,
    /// Games we led at some point and lost.
    pub blown_leads: u32,
    pub games_led: u32,
}

fn segment_index(t: Seconds) -> usize {
    let regulation = SEGMENT_SECONDS * REGULATION_SEGMENTS as f64;
    if t.0 >= regulation {
        return REGULATION_SEGMENTS;
    }
    crate::stats::describe::floor_index(t.0.max(0.0) / SEGMENT_SECONDS).min(REGULATION_SEGMENTS - 1)
}

fn segments(games: &[&Game]) -> Vec<Segment> {
    let mut result: Vec<Segment> = (0..=REGULATION_SEGMENTS)
        .map(|i| {
            let start = u32::try_from(i).unwrap_or(u32::MAX) * 5;
            Segment {
                start_minute: start,
                end_minute: (i < REGULATION_SEGMENTS).then_some(start + 5),
                goals_for: 0,
                goals_against: 0,
            }
        })
        .collect();
    for goal in games.iter().flat_map(|g| g.goals.iter()) {
        let segment = &mut result[segment_index(goal.time)];
        match goal.scored_by {
            Team::Us => segment.goals_for += 1,
            Team::Them => segment.goals_against += 1,
        }
    }
    if result.last().is_some_and(|s| s.goals_for + s.goals_against == 0) {
        result.pop();
    }
    result
}

/// Merged `(start, end]` stretches following each goal by `after`, clipped to the game.
fn windows(goals: &[Goal], after: Team, length: Seconds) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = goals
        .iter()
        .filter(|g| g.scored_by == after)
        .map(|g| (g.time.0, (g.time.0 + RESPONSE_WINDOW).min(length.0)))
        .filter(|(start, end)| end > start)
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Goal counts and minutes inside and outside the windows, for one game.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Exposure {
    windows: u32,
    inside: u32,
    inside_seconds: f64,
    outside: u32,
    outside_seconds: f64,
}

fn exposure(game: &Game, after: Team, scorer: Team) -> Exposure {
    let spans = windows(&game.goals, after, game.length);
    let inside_seconds: f64 = spans.iter().map(|(a, b)| b - a).sum();
    let is_inside = |t: f64| spans.iter().any(|(a, b)| *a < t && t <= *b);
    let (inside, outside) = game
        .goals
        .iter()
        .filter(|g| g.scored_by == scorer)
        .fold((0, 0), |(i, o), g| if is_inside(g.time.0) { (i + 1, o) } else { (i, o + 1) });
    Exposure {
        windows: u32::try_from(game.goals.iter().filter(|g| g.scored_by == after).count()).unwrap_or(u32::MAX),
        inside,
        inside_seconds,
        outside,
        outside_seconds: (game.length.0 - inside_seconds).max(0.0),
    }
}

fn response(games: &[&Game], after: Team, scorer: Team) -> Response {
    let total = games.iter().fold(Exposure::default(), |acc, g| {
        let e = exposure(g, after, scorer);
        Exposure {
            windows: acc.windows + e.windows,
            inside: acc.inside + e.inside,
            inside_seconds: acc.inside_seconds + e.inside_seconds,
            outside: acc.outside + e.outside,
            outside_seconds: acc.outside_seconds + e.outside_seconds,
        }
    });
    let goals = total.inside + total.outside;
    let seconds = total.inside_seconds + total.outside_seconds;
    let testable = goals >= MIN_GOALS_FOR_RESPONSE && total.inside_seconds > 0.0 && total.outside_seconds > 0.0;
    let rate_ratio = (total.inside_seconds > 0.0 && total.outside > 0)
        .then(|| (f64::from(total.inside) / total.inside_seconds) / (f64::from(total.outside) / total.outside_seconds));
    Response {
        after,
        scorer,
        windows: total.windows,
        goals_in_windows: total.inside,
        minutes_in_windows: total.inside_seconds / 60.0,
        goals_elsewhere: total.outside,
        minutes_elsewhere: total.outside_seconds / 60.0,
        rate_ratio,
        p: testable.then(|| {
            binomial_exact_two_sided(u64::from(total.inside), u64::from(goals), total.inside_seconds / seconds)
        }),
    }
}

/// Our lead after each goal, in time order.
fn leads(game: &Game) -> impl Iterator<Item = (Seconds, i64)> + '_ {
    game.goals
        .iter()
        .map(|g| (g.time, i64::from(g.score_after.0) - i64::from(g.score_after.1)))
}

fn lead_at(game: &Game, t: Seconds) -> Lead {
    let diff = leads(game).filter(|(time, _)| time.0 <= t.0).last().map_or(0, |(_, d)| d);
    match diff.signum() {
        1 => Lead::Leading,
        -1 => Lead::Trailing,
        _ => Lead::Tied,
    }
}

fn by_lead(games: &[&Game]) -> Vec<LeadRecord> {
    let mut rows: Vec<LeadRecord> = Vec::new();
    for after_period in [1, 2] {
        for lead in [Lead::Leading, Lead::Tied, Lead::Trailing] {
            let mut record = Record::default();
            for game in games.iter().filter(|g| lead_at(g, Seconds(f64::from(after_period) * PERIOD_SECONDS)) == lead) {
                record.add(outcome(game));
            }
            rows.push(LeadRecord { after_period, lead, record });
        }
    }
    rows
}

fn momentum_tests(responses: &[Response], tests: &mut Vec<TestRow>) {
    let family = "Momentum after goals";
    for r in responses {
        let who = |team: Team| if team == Team::Us { "we" } else { "they" };
        let question = format!(
            "After {} score, do {} score more (or less) than usual in the next {:.0} minutes?",
            who(r.after),
            who(r.scorer),
            RESPONSE_WINDOW / 60.0
        );
        let method = "Exact conditional Poisson test (binomial split of goals by time inside vs outside the windows)";
        let n = format!("{} goals, {} windows", r.goals_in_windows + r.goals_elsewhere, r.windows);
        let Some(p) = r.p else {
            tests.push(TestRow::not_enough(family, &question, method, n, "needs 8+ goals by that team"));
            continue;
        };
        tests.push(TestRow {
            family: family.to_owned(),
            question,
            method: method.to_owned(),
            statistic_label: "Goals in windows".to_owned(),
            statistic: Some(f64::from(r.goals_in_windows)),
            df: None,
            p: Some(p),
            p_adjusted: None,
            effect_label: "Rate ratio (windows ÷ rest of the game)".to_owned(),
            effect: r.rate_ratio,
            ci: None,
            n,
            secondary: None,
            verdict: Verdict::from_adjusted_p(Some(p)),
            plain: r.rate_ratio.map_or_else(
                || "No goals outside the windows to compare with.".to_owned(),
                |ratio| format!("Goals come {ratio:.1}× as fast in those {:.0} minutes as the rest of the time.", RESPONSE_WINDOW / 60.0),
            ),
            assumptions: "Goals arrive at a steady rate apart from the windows; games are independent.".to_owned(),
        });
    }
}

#[must_use]
pub fn timing(context: &Context<'_>, tests: &mut Vec<TestRow>) -> TimingReport {
    let games = &context.scope;
    let responses: Vec<Response> = [(Team::Us, Team::Us), (Team::Us, Team::Them), (Team::Them, Team::Us), (Team::Them, Team::Them)]
        .into_iter()
        .map(|(after, scorer)| response(games, after, scorer))
        .collect();
    momentum_tests(&responses, tests);
    let (mut scored_first, mut conceded_first, mut scoreless) = (Record::default(), Record::default(), Record::default());
    let (mut comebacks, mut games_trailed, mut blown_leads, mut games_led) = (0, 0, 0, 0);
    for game in games {
        let result = outcome(game);
        match game.goals.first().map(|g| g.scored_by) {
            Some(Team::Us) => scored_first.add(result),
            Some(Team::Them) => conceded_first.add(result),
            None => scoreless.add(result),
        }
        if leads(game).any(|(_, d)| d < 0) {
            games_trailed += 1;
            comebacks += u32::from(result != Outcome::Loss);
        }
        if leads(game).any(|(_, d)| d > 0) {
            games_led += 1;
            blown_leads += u32::from(matches!(result, Outcome::Loss | Outcome::OvertimeLoss));
        }
    }
    TimingReport {
        segments: segments(games),
        responses,
        response_window_minutes: RESPONSE_WINDOW / 60.0,
        scored_first,
        conceded_first,
        scoreless,
        by_lead: by_lead(games),
        comebacks,
        games_trailed,
        blown_leads,
        games_led,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Strength;

    fn goal(minute: f64, scored_by: Team, score_after: (u32, u32)) -> Goal {
        Goal {
            time: Seconds(minute * 60.0),
            scored_by,
            strength: Strength::Even,
            score_after,
            on_ice: Vec::new(),
        }
    }

    #[test]
    fn overlapping_windows_are_merged_and_clipped_to_the_game() {
        let goals = [goal(10.0, Team::Us, (1, 0)), goal(11.0, Team::Us, (2, 0)), goal(59.5, Team::Us, (3, 0))];
        let spans = windows(&goals, Team::Us, Seconds(3600.0));
        assert_eq!(spans, vec![(600.0, 780.0), (3570.0, 3600.0)]);
    }

    #[test]
    fn a_goal_is_not_counted_in_its_own_window() {
        let goals = [goal(10.0, Team::Us, (1, 0)), goal(11.0, Team::Them, (1, 1)), goal(30.0, Team::Them, (1, 2))];
        let game = test_game(&goals);
        let after_us = exposure(&game, Team::Us, Team::Us);
        assert_eq!((after_us.inside, after_us.outside), (0, 1));
        let their_reply = exposure(&game, Team::Us, Team::Them);
        assert_eq!((their_reply.inside, their_reply.outside), (1, 1));
        assert!((their_reply.inside_seconds - RESPONSE_WINDOW).abs() < 1e-9);
    }

    #[test]
    fn late_regulation_and_overtime_goals_land_in_their_own_stretches() {
        assert_eq!(segment_index(Seconds(3599.0)), 11);
        assert_eq!(segment_index(Seconds(3650.0)), REGULATION_SEGMENTS);
        assert_eq!(segment_index(Seconds(0.0)), 0);
    }

    #[test]
    fn the_lead_after_a_period_ignores_later_goals() {
        let game = test_game(&[goal(5.0, Team::Them, (0, 1)), goal(25.0, Team::Us, (1, 1)), goal(45.0, Team::Us, (2, 1))]);
        assert_eq!(lead_at(&game, Seconds(PERIOD_SECONDS)), Lead::Trailing);
        assert_eq!(lead_at(&game, Seconds(2.0 * PERIOD_SECONDS)), Lead::Tied);
    }

    fn test_game(goals: &[Goal]) -> Game {
        Game {
            id: crate::model::GameId("g".into()),
            date: crate::model::Date { year: 2026, month: 9, day: 1 },
            team: crate::model::TeamName("US".into()),
            opponent: crate::model::TeamName("THEM".into()),
            goals_for: goals.iter().filter(|g| g.scored_by == Team::Us).count().try_into().unwrap(),
            goals_against: goals.iter().filter(|g| g.scored_by == Team::Them).count().try_into().unwrap(),
            players: Vec::new(),
            units: Vec::new(),
            passes: None,
            goals: goals.to_vec(),
            advantages: Vec::new(),
            summary: crate::model::TeamSummary::default(),
            opponent_summary: crate::model::TeamSummary::default(),
            team_stats: Vec::new(),
            shot_zones_against: Vec::new(),
            matchups: Vec::new(),
            charted_shots: Vec::new(),
            charted_shots_against: Vec::new(),
            faceoff_spots: Vec::new(),
            goal_plays: Vec::new(),
            opponent_skaters: Vec::new(),
            length: Seconds(3600.0),
            warnings: Vec::new(),
        }
    }
}
