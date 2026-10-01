//! How the team plays with and without the puck, from the team counts in the event export
//! pooled over games, and the practice focus areas those counts point to.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::share_pct;
use crate::analysis::team::TeamReport;
use crate::model::{BattleArea, TeamPlay};

/// Team counts for us and them, summed over the games in scope.
#[derive(Debug, Clone, Default, PartialEq)]
struct Pooled {
    ours: TeamPlay,
    theirs: TeamPlay,
    scoring_chances: (u32, u32),
    /// Seconds with the puck, from InStat's match reports, for the games that have one.
    possession: (f64, f64),
}

fn pool(context: &Context<'_>) -> Pooled {
    let mut pooled = Pooled::default();
    for game in &context.scope {
        pooled.ours.add(game.summary.play);
        pooled.theirs.add(game.opponent_summary.play);
        pooled.scoring_chances.0 += game.summary.scoring_chances.0;
        pooled.scoring_chances.1 += game.opponent_summary.scoring_chances.0;
        pooled.possession.0 += game.summary.possession_time.0;
        pooled.possession.1 += game.opponent_summary.possession_time.0;
    }
    pooled
}

fn pair(ours: u32, theirs: u32) -> (f64, f64) {
    (f64::from(ours), f64::from(theirs))
}

/// One team count, picked out of [`TeamPlay`].
type Pick = fn(&TeamPlay) -> u32;

impl Pooled {
    fn of(&self, pick: Pick) -> (f64, f64) {
        pair(pick(&self.ours), pick(&self.theirs))
    }
}

const fn entries(play: &TeamPlay) -> u32 {
    play.entries.total()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Better {
    Higher,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Measure {
    Count,
    Seconds,
}

/// One aspect of play compared with the opponent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub label: String,
    pub ours: f64,
    pub theirs: f64,
    /// Our share of the two, in percent.
    pub share: Option<f64>,
    pub better: Better,
    pub measure: Measure,
    /// e.g. how often these attacks produced a shot, for (us, them).
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparisonGroup {
    pub title: String,
    pub items: Vec<Comparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FocusArea {
    pub area: String,
    /// What the numbers say, in one sentence.
    pub evidence: String,
    pub suggestion: String,
    /// Percentage points worse than the benchmark (0 when at or better).
    pub gap: f64,
    /// Events behind the figure, so the UI can flag thin evidence.
    pub events: f64,
    pub verdict: FocusVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FocusVerdict {
    WorkOn,
    Watch,
    Strength,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StyleReport {
    pub groups: Vec<ComparisonGroup>,
    pub focus: Vec<FocusArea>,
}

/// A comparison, unless neither team has any (the count isn't in the games' files).
fn comparison(label: &str, (ours, theirs): (f64, f64), better: Better, measure: Measure) -> Option<Comparison> {
    (ours + theirs > 0.0).then(|| Comparison {
        label: label.to_owned(),
        ours,
        theirs,
        share: share_pct(ours, theirs),
        better,
        measure,
        note: None,
    })
}

fn rate(part: f64, whole: f64) -> Option<f64> {
    (whole > 0.0).then(|| 100.0 * part / whole)
}

/// "57% vs 86% …" from each team's (part, whole) counts.
fn rate_note(part: (f64, f64), whole: (f64, f64), what: &str) -> Option<String> {
    Some(format!("{:.0}% vs {:.0}% {what}", rate(part.0, whole.0)?, rate(part.1, whole.1)?))
}

fn with_note(item: Comparison, note: Option<String>) -> Comparison {
    Comparison { note, ..item }
}

fn attack_group(pooled: &Pooled) -> ComparisonGroup {
    let chances = pair(pooled.scoring_chances.0, pooled.scoring_chances.1);
    ComparisonGroup {
        title: "How we attack".to_owned(),
        items: [
            comparison("Scoring chances", chances, Better::Higher, Measure::Count),
            comparison("Passes into the slot", pooled.of(|p| p.passes_to_slot), Better::Higher, Measure::Count),
            comparison("Zone entries", pooled.of(entries), Better::Higher, Measure::Count)
                .map(|c| with_note(c, rate_note(chances, pooled.of(entries), "of entries led to a scoring chance"))),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn entry_group(pooled: &Pooled) -> ComparisonGroup {
    let total = pooled.of(entries);
    let kinds: [(&str, Pick); 3] =
        [("Carried in", |p| p.entries.carry), ("Passed in", |p| p.entries.pass), ("Dumped in", |p| p.entries.dump_in)];
    ComparisonGroup {
        title: "How we enter their zone".to_owned(),
        items: kinds
            .iter()
            .filter_map(|(label, pick)| {
                let counts = pooled.of(*pick);
                comparison(label, counts, Better::Higher, Measure::Count).map(|c| with_note(c, rate_note(counts, total, "of entries")))
            })
            .collect(),
    }
}

fn breakout_group(pooled: &Pooled) -> ComparisonGroup {
    let breakouts = pooled.of(|p| p.breakouts);
    ComparisonGroup {
        title: "How we break out".to_owned(),
        items: [
            comparison("Breakouts", breakouts, Better::Higher, Measure::Count),
            comparison("Controlled breakouts", pooled.of(|p| p.controlled_breakouts), Better::Higher, Measure::Count)
                .map(|c| with_note(c, rate_note(pooled.of(|p| p.controlled_breakouts), breakouts, "of breakouts"))),
            comparison("Dumped out", pooled.of(|p| p.dump_outs), Better::Lower, Measure::Count),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn puck_group(pooled: &Pooled) -> ComparisonGroup {
    ComparisonGroup {
        title: "Puck management".to_owned(),
        items: [
            comparison("Time with the puck", pooled.possession, Better::Higher, Measure::Seconds),
            comparison("Puck recoveries", pooled.of(|p| p.puck_recoveries), Better::Higher, Measure::Count),
            comparison("Recoveries in their zone", pooled.of(|p| p.puck_recoveries_offensive_zone), Better::Higher, Measure::Count),
            comparison("Puck losses", pooled.of(|p| p.puck_losses), Better::Lower, Measure::Count),
            comparison("Puck losses in own zone", pooled.of(|p| p.puck_losses_defensive_zone), Better::Lower, Measure::Count),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

/// Turns (ours, theirs, our share %) into the evidence sentence.
type Describe = Box<dyn Fn(f64, f64, f64) -> String>;

/// A candidate focus area: our and the opponent's count, judged against `benchmark`% for us.
struct Candidate {
    area: &'static str,
    suggestion: &'static str,
    values: Option<(f64, f64)>,
    better: Better,
    benchmark: f64,
    min_events: f64,
    describe: Describe,
}

const MIN_EVENTS: f64 = 8.0;
const CLEAR_GAP: f64 = 8.0;
/// Fewer events than this are too few to call anything more than "watch".
const MIN_VERDICT_EVENTS: f64 = 10.0;

/// (focus area, where on the ice, practice idea) for each battle area.
const fn battle_text(area: BattleArea) -> (&'static str, &'static str, &'static str) {
    match area {
        BattleArea::OwnSlot => (
            "Net-front battles in our slot",
            "in front of our net",
            "Box-outs and stick-on-puck in front of our net; net-front 1-on-1s.",
        ),
        BattleArea::BehindOwnGoal => (
            "Battles behind our net",
            "behind our net",
            "D retrievals under pressure: shoulder checks, wheel or reverse, centre support low.",
        ),
        BattleArea::OwnCorners => (
            "Corner battles in our zone",
            "in our corners",
            "1-on-1 and 2-on-1 corner drills; body position and winning the wall.",
        ),
        BattleArea::OwnBlueLine => (
            "Battles at our blue line",
            "along our blue line",
            "Winger wall work on breakouts: chip-outs and protecting the puck on the boards.",
        ),
        BattleArea::NeutralZone => (
            "Neutral-zone battles",
            "in the neutral zone",
            "Loose-puck races and support in the neutral zone.",
        ),
        BattleArea::OppBlueLine => (
            "Battles at their blue line",
            "along their blue line",
            "Holding the line: D pinches, keeping pucks in, winning rims.",
        ),
        BattleArea::OppCorners => (
            "Corner battles in their zone",
            "in their corners",
            "Forecheck and cycle: 2-on-1 down low, protect the puck and win the wall.",
        ),
        BattleArea::BehindOppGoal => (
            "Battles behind their net",
            "behind their net",
            "Below-the-goal-line play: first player on the puck, second player in support.",
        ),
        BattleArea::OppSlot => (
            "Net-front battles in their slot",
            "in front of their net",
            "Net-front presence: screens, tips and winning rebounds.",
        ),
    }
}

fn battle_candidates(team: &TeamReport) -> impl Iterator<Item = Candidate> + '_ {
    team.battle_areas.iter().map(|a| {
        let (area, place, suggestion) = battle_text(a.area);
        Candidate {
            area,
            suggestion,
            values: Some((f64::from(a.won), f64::from(a.battles.saturating_sub(a.won)))),
            better: Better::Higher,
            benchmark: 50.0,
            min_events: MIN_EVENTS,
            describe: Box::new(move |o, t, s| format!("Won {o:.0} of {:.0} battles {place} ({s:.0}%).", o + t)),
        }
    })
}

fn candidates(pooled: &Pooled, team: &TeamReport) -> Vec<Candidate> {
    let faceoffs = |zone: &str| {
        team.faceoffs
            .iter()
            .find(|z| z.zone == zone)
            .map(|z| (f64::from(z.won), f64::from(z.lost)))
    };
    let even = |area, suggestion, values, better, describe: fn(f64, f64, f64) -> String| Candidate {
        area,
        suggestion,
        values,
        better,
        benchmark: 50.0,
        min_events: MIN_EVENTS,
        describe: Box::new(describe),
    };
    let pp_goals = f64::from(team.power_play_goals);
    let pp_misses = f64::from(team.power_play_chances.saturating_sub(team.power_play_goals));
    let kills = f64::from(team.times_short_handed.saturating_sub(team.power_play_goals_against));
    let conceded = f64::from(team.power_play_goals_against);
    [
        even(
            "Faceoffs in our zone",
            "Centre draw reps, plus wingers jumping in on D-zone faceoffs.",
            faceoffs("Defensive zone"),
            Better::Higher,
            |o, t, s| format!("Won {o:.0} of {:.0} draws in our zone ({s:.0}%).", o + t),
        ),
        even(
            "Faceoffs in their zone",
            "Offensive-zone faceoff plays and centre technique.",
            faceoffs("Offensive zone"),
            Better::Higher,
            |o, t, s| format!("Won {o:.0} of {:.0} draws in their zone ({s:.0}%).", o + t),
        ),
        even(
            "Getting to the scoring areas",
            "Net drives and slot shooting; low-to-high plays that open the slot.",
            Some(pair(pooled.scoring_chances.0, pooled.scoring_chances.1)),
            Better::Higher,
            |o, t, s| format!("{o:.0} scoring chances for, {t:.0} against ({s:.0}%)."),
        ),
        even(
            "Breaking out cleanly",
            "Breakouts under pressure: D-to-D, wall support, first pass.",
            Some(pooled.of(|p| p.puck_losses_defensive_zone)),
            Better::Lower,
            |o, t, _| format!("Lost the puck {o:.0} times in our zone; they lost it {t:.0} times in theirs."),
        ),
        even(
            "Winning pucks back in their zone",
            "Forecheck pressure and support so the second player wins the loose puck.",
            Some(pooled.of(|p| p.puck_recoveries_offensive_zone)),
            Better::Higher,
            |o, t, s| format!("{o:.0} recoveries in their zone vs {t:.0} by them in ours ({s:.0}%)."),
        ),
        Candidate {
            area: "Power play",
            suggestion: "Power-play entries and set plays.",
            values: (team.power_play_chances > 0).then_some((pp_goals, pp_misses)),
            better: Better::Higher,
            benchmark: 20.0,
            min_events: 1.0,
            describe: Box::new(|o, t, _| format!("{o:.0} goals on {:.0} power plays.", o + t)),
        },
        Candidate {
            area: "Penalty kill",
            suggestion: "PK structure, clears and blocking shooting lanes.",
            values: (team.times_short_handed > 0).then_some((kills, conceded)),
            better: Better::Higher,
            benchmark: 80.0,
            min_events: 1.0,
            describe: Box::new(|o, t, _| format!("Killed {o:.0} of {:.0} penalties.", o + t)),
        },
    ]
    .into_iter()
    .chain(battle_candidates(team))
    .collect()
}

fn assess(candidate: &Candidate) -> Option<FocusArea> {
    let (ours, theirs) = candidate.values?;
    let events = ours + theirs;
    if events < candidate.min_events {
        return None;
    }
    let share = share_pct(ours, theirs)?;
    let shortfall = match candidate.better {
        Better::Higher => candidate.benchmark - share,
        Better::Lower => share - candidate.benchmark,
    };
    let verdict = if events < MIN_VERDICT_EVENTS {
        FocusVerdict::Watch
    } else if shortfall >= CLEAR_GAP {
        FocusVerdict::WorkOn
    } else if shortfall <= -CLEAR_GAP {
        FocusVerdict::Strength
    } else {
        FocusVerdict::Watch
    };
    Some(FocusArea {
        area: candidate.area.to_owned(),
        evidence: (candidate.describe)(ours, theirs, share),
        suggestion: candidate.suggestion.to_owned(),
        gap: shortfall.max(0.0),
        events,
        verdict,
    })
}

/// Areas sorted from most to least in need of work.
fn focus_areas(pooled: &Pooled, team: &TeamReport) -> Vec<FocusArea> {
    let mut areas: Vec<FocusArea> = candidates(pooled, team).iter().filter_map(assess).collect();
    areas.sort_by(|a, b| {
        (a.verdict as u8)
            .cmp(&(b.verdict as u8))
            .then_with(|| b.gap.total_cmp(&a.gap))
            .then_with(|| a.area.cmp(&b.area))
    });
    areas
}

#[must_use]
pub fn style(context: &Context<'_>, team: &TeamReport) -> StyleReport {
    let pooled = pool(context);
    StyleReport {
        groups: vec![attack_group(&pooled), entry_group(&pooled), breakout_group(&pooled), puck_group(&pooled)],
        focus: focus_areas(&pooled, team),
    }
}
