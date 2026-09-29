//! How the team plays with and without the puck, from InStat's possession, entry, attack and
//! turnover counts pooled over games, and the practice focus areas those counts point to.
//!
//! The PDFs give counts per game, not event sequences, so whistle-to-whistle chains cannot be
//! rebuilt; attack types (positional vs counter-attack) and entry types are the closest
//! categories available.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::share_pct;
use crate::analysis::team::TeamReport;
use crate::cell::Cell;
use crate::model::{BattleArea, CellValue};

const ATTACKS: &str = "OZ possession";
const LOSSES: &str = "Puck losses";
const RECOVERIES: &str = "Puck recoveries";
const POSSESSION: &str = "Puck possessions at even";
const SHOTS: &str = "Shots and goals";

/// A team stat's primary number for (us, them), summed over games in scope.
#[derive(Debug, Clone, PartialEq)]
struct PooledStat {
    group: String,
    label: String,
    ours: f64,
    theirs: f64,
}

impl PooledStat {
    const fn values(&self) -> (f64, f64) {
        (self.ours, self.theirs)
    }
}

/// The number InStat leads a cell with; percentages are skipped since they don't sum.
fn count(value: &CellValue) -> Option<f64> {
    if value.text == "—" {
        return Some(0.0);
    }
    match Cell::parse(&value.text) {
        Cell::Empty => Some(0.0),
        Cell::Int(n) => Some(n as f64),
        Cell::CountShare(n, _) | Cell::Ratio(n, _) | Cell::Triple(n, _, _) => Some(f64::from(n)),
        Cell::Clock(s) | Cell::ClockShare(s, _) => Some(f64::from(s)),
        Cell::Decimal(d) => Some(d),
        Cell::Percent(_) | Cell::Pair(..) | Cell::Text(_) => None,
    }
}

/// Keyed by (group, label, occurrence in group) because InStat repeats labels like
/// "With shot, %" within one group.
fn pool(context: &Context<'_>) -> Vec<PooledStat> {
    let mut sums: BTreeMap<(String, String, usize), (usize, f64, f64)> = BTreeMap::new();
    for game in &context.scope {
        let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for row in &game.team_stats {
            let occurrence = seen.entry((&row.group, &row.label)).or_default();
            let key = (row.group.clone(), row.label.clone(), *occurrence);
            *occurrence += 1;
            let (Some(ours), Some(theirs)) = (count(&row.ours), count(&row.theirs)) else {
                continue;
            };
            let first_seen = sums.len();
            let entry = sums.entry(key).or_insert((first_seen, 0.0, 0.0));
            entry.1 += ours;
            entry.2 += theirs;
        }
    }
    let mut pooled: Vec<(usize, PooledStat)> = sums
        .into_iter()
        .map(|((group, label, _), (order, ours, theirs))| {
            (order, PooledStat { group, label, ours, theirs })
        })
        .collect();
    pooled.sort_by_key(|(order, _)| *order);
    pooled.into_iter().map(|(_, stat)| stat).collect()
}

fn find<'a>(stats: &'a [PooledStat], group_prefix: &str, label: &str, nth: usize) -> Option<&'a PooledStat> {
    stats
        .iter()
        .filter(|s| s.group.starts_with(group_prefix) && s.label == label)
        .nth(nth)
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

fn comparison(label: &str, stat: Option<&PooledStat>, better: Better, measure: Measure) -> Option<Comparison> {
    let stat = stat?;
    Some(Comparison {
        label: label.to_owned(),
        ours: stat.ours,
        theirs: stat.theirs,
        share: share_pct(stat.ours, stat.theirs),
        better,
        measure,
        note: None,
    })
}

fn rate(part: f64, whole: f64) -> Option<f64> {
    (whole > 0.0).then(|| 100.0 * part / whole)
}

/// "57% vs 86% …" from (part, whole) counts for each team.
fn rate_note(part: Option<&PooledStat>, whole: Option<&PooledStat>, what: &str) -> Option<String> {
    let (part, whole) = (part?, whole?);
    let ours = rate(part.ours, whole.ours)?;
    let theirs = rate(part.theirs, whole.theirs)?;
    Some(format!("{ours:.0}% vs {theirs:.0}% {what}"))
}

fn with_note(item: Comparison, note: Option<String>) -> Comparison {
    Comparison { note, ..item }
}

fn attack_groups(stats: &[PooledStat]) -> ComparisonGroup {
    let positional = find(stats, ATTACKS, "Positional attacks", 0);
    let counters = find(stats, ATTACKS, "Counterattacks", 0);
    ComparisonGroup {
        title: "How we attack".to_owned(),
        items: [
            comparison("Positional attacks (set up in their zone)", positional, Better::Higher, Measure::Count)
                .map(|c| with_note(c, rate_note(find(stats, ATTACKS, "With shot, %", 0), positional, "got a shot"))),
            comparison("Counter-attacks (quick transition)", counters, Better::Higher, Measure::Count)
                .map(|c| with_note(c, rate_note(find(stats, ATTACKS, "With shot, %", 1), counters, "got a shot"))),
            comparison("Possessions in their zone", find(stats, ATTACKS, "Possessions in offensive zone", 0), Better::Higher, Measure::Count),
            comparison("Shots from scoring-chance areas", find(stats, SHOTS, "Shots from a scoring chance area", 0), Better::Higher, Measure::Count),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn entry_group(stats: &[PooledStat]) -> ComparisonGroup {
    let entries: Vec<Option<&PooledStat>> = ["Entries by stickhandling", "Entries by pass", "Entries by dump in"]
        .iter()
        .map(|label| find(stats, ATTACKS, label, 0))
        .collect();
    let total = |pick: fn(&PooledStat) -> f64| entries.iter().flatten().map(|s| pick(s)).sum::<f64>();
    let (ours_total, theirs_total) = (total(|s| s.ours), total(|s| s.theirs));
    let share_note = |stat: Option<&PooledStat>| {
        let stat = stat?;
        Some(format!(
            "{:.0}% vs {:.0}% of entries",
            rate(stat.ours, ours_total)?,
            rate(stat.theirs, theirs_total)?
        ))
    };
    let labels = ["Carried in", "Passed in", "Dumped in"];
    ComparisonGroup {
        title: "How we enter their zone".to_owned(),
        items: labels
            .iter()
            .zip(&entries)
            .filter_map(|(label, stat)| {
                comparison(label, *stat, Better::Higher, Measure::Count).map(|c| with_note(c, share_note(*stat)))
            })
            .collect(),
    }
}

fn puck_group(stats: &[PooledStat]) -> ComparisonGroup {
    ComparisonGroup {
        title: "Puck management".to_owned(),
        items: [
            comparison("Time with the puck", find(stats, POSSESSION, "Puck possessions", 0), Better::Higher, Measure::Seconds),
            comparison("Takeaways", find(stats, RECOVERIES, "After takeaways", 0), Better::Higher, Measure::Count),
            comparison("Puck losses in own zone", find(stats, LOSSES, "Defensive zone", 0), Better::Lower, Measure::Count),
            comparison("Giveaways that flipped possession", find(stats, LOSSES, "Giveaways -> transition of possession", 0), Better::Lower, Measure::Count),
            comparison("Icings", find(stats, LOSSES, "Icings", 0), Better::Lower, Measure::Count),
            comparison("Offsides", find(stats, LOSSES, "Offsides", 0), Better::Lower, Measure::Count),
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

fn candidates(stats: &[PooledStat], team: &TeamReport) -> Vec<Candidate> {
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
            find(stats, SHOTS, "Shots from a scoring chance area", 0).map(PooledStat::values),
            Better::Higher,
            |o, t, s| format!("{o:.0} scoring-area shots for, {t:.0} against ({s:.0}%)."),
        ),
        even(
            "Breaking out cleanly",
            "Breakouts under pressure: D-to-D, wall support, first pass.",
            find(stats, LOSSES, "Defensive zone", 0).map(PooledStat::values),
            Better::Lower,
            |o, t, _| format!("Lost the puck {o:.0} times in our zone; they lost it {t:.0} times in theirs."),
        ),
        even(
            "Sustained pressure in their zone",
            "Cycling, puck protection and D activating from the point.",
            find(stats, ATTACKS, "Possessions in offensive zone", 0).map(PooledStat::values),
            Better::Higher,
            |o, t, s| format!("{o:.0} possessions in their zone vs {t:.0} in ours ({s:.0}%)."),
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
fn focus_areas(stats: &[PooledStat], team: &TeamReport) -> Vec<FocusArea> {
    let mut areas: Vec<FocusArea> = candidates(stats, team).iter().filter_map(assess).collect();
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
        groups: vec![attack_groups(&pooled), entry_group(&pooled), puck_group(&pooled)],
        focus: focus_areas(&pooled, team),
    }
}
