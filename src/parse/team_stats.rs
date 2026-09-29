//! The TEAMS STATS page: four label/value blocks per section, one value column per team.

use crate::cell::Cell;
use crate::error::Error;
use crate::layout::{self, Line};
use crate::model::{CellValue, Seconds, StatEntry, TeamSummary};
use crate::parse::common::{LINE_TOLERANCE, PAGE_RIGHT};
use crate::parse::diagrams;
use crate::pdf::{Page, Rect};

const SECTION: &str = "team stats";

/// All labelled values for both teams, in page order.
#[derive(Debug, Clone, Default)]
pub struct TeamStatsPage {
    pub abbreviations: [String; 2],
    pub entries: [Vec<StatEntry>; 2],
    /// The "faceoffs by zones" rink as drawn (see [`diagrams::faceoff_dots`]).
    pub faceoff_dots: Option<[(u32, u32); 9]>,
}

/// The "faceoffs by zones" rink: between its title and the zone labels under it, and
/// between the diagrams beside it.
fn faceoff_rink(lines: &[Line<'_>]) -> Option<Rect> {
    let (title_y, title_x) = lines.iter().find_map(|l| l.find_phrase("FACEOFFS BY ZONES").map(|x| (l.y, x)))?;
    let center = title_x + 30.0;
    let bottom = lines
        .iter()
        .filter(|l| l.y > title_y)
        .find_map(|l| l.find_phrase("NEUTRAL").filter(|x| (x - center).abs() < 60.0).map(|_| l.y))?;
    Some(Rect {
        x0: center - 100.0,
        x1: center + 100.0,
        top: title_y + 4.0,
        bottom: bottom - 4.0,
    })
}

struct Block {
    label_x: f64,
    first_value_x: f64,
    second_value_x: f64,
    end_x: f64,
    title: String,
}

fn is_abbreviation_line(line: &Line<'_>) -> bool {
    line.words.len() >= 4
        && line
            .words
            .iter()
            .all(|w| w.text.len() <= 5 && w.text.chars().all(|c| c.is_ascii_uppercase()))
}

/// Block i's label column starts at the first title word right of block i-1's values.
fn blocks(title_line: Option<&Line<'_>>, abbreviation_line: &Line<'_>) -> Vec<Block> {
    let value_xs: Vec<f64> = abbreviation_line.words.iter().map(|w| w.x0).collect();
    let title_xs: Vec<f64> = title_line
        .map(|line| line.words.iter().map(|w| w.x0).collect())
        .unwrap_or_default();
    let mut result: Vec<Block> = Vec::new();
    for pair in value_xs.as_chunks::<2>().0 {
        let previous_end = result.last().map_or(0.0, |b: &Block| b.second_value_x);
        let label_x = title_xs
            .iter()
            .copied()
            .find(|x| *x > previous_end + 5.0 && *x < pair[0])
            .unwrap_or(previous_end + 5.0);
        result.push(Block {
            label_x,
            first_value_x: pair[0],
            second_value_x: pair[1],
            end_x: PAGE_RIGHT,
            title: String::new(),
        });
    }
    for i in 1..result.len() {
        result[i - 1].end_x = result[i].label_x;
    }
    if let Some(line) = title_line {
        for block in &mut result {
            block.title = line.text_between(block.label_x - 1.0, block.end_x - 1.0);
        }
    }
    result
}

/// Labels contain letters and are not themselves values (e.g. "1st period", not "9 —4").
fn is_label(text: &str) -> bool {
    text.chars().any(char::is_alphabetic) && matches!(Cell::parse(text), Cell::Text(_))
}

fn has_value(text: &str) -> bool {
    !text.is_empty() && !matches!(Cell::parse(text), Cell::Text(_))
}

pub fn parse(page: &Page) -> Result<TeamStatsPage, Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let mut result = TeamStatsPage::default();
    let mut current: Vec<Block> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if is_abbreviation_line(line) {
            if result.abbreviations[0].is_empty() {
                result.abbreviations = [line.words[0].text.clone(), line.words[1].text.clone()];
            }
            current = blocks(index.checked_sub(1).map(|i| &lines[i]), line);
            continue;
        }
        for block in &current {
            let slack = 2.0;
            let label = line.text_between(block.label_x - slack, block.first_value_x - slack);
            if !is_label(&label) {
                continue;
            }
            let first =
                line.text_between(block.first_value_x - slack, block.second_value_x - slack);
            let second = line.text_between(block.second_value_x - slack, block.end_x - slack);
            if !has_value(&first) && !has_value(&second) {
                continue;
            }
            for (side, value) in [first, second].iter().enumerate() {
                result.entries[side].push(StatEntry {
                    group: block.title.clone(),
                    label: label.clone(),
                    value: CellValue::from_text(value),
                });
            }
        }
    }
    if result.abbreviations[0].is_empty() {
        return Err(Error::parse(SECTION, "no team abbreviation header"));
    }
    result.faceoff_dots = faceoff_rink(&lines).and_then(|region| diagrams::faceoff_dots(&page.words, &region));
    Ok(result)
}

fn find<'a>(entries: &'a [StatEntry], group_prefix: &str, label: &str) -> Option<Cell> {
    entries
        .iter()
        .find(|e| e.group.starts_with(group_prefix) && e.label == label)
        .map(|e: &'a StatEntry| Cell::parse(&e.value.text))
}

fn find_any(entries: &[StatEntry], label: &str) -> Option<Cell> {
    find(entries, "", label)
}

/// Extracts the numbers the analysis uses; missing fields stay at their defaults.
#[must_use]
pub fn summary(entries: &[StatEntry]) -> TeamSummary {
    let shots_group = "Shots and goals";
    let triple = |label: &str| {
        find(entries, shots_group, label)
            .and_then(|c| c.triple())
            .unwrap_or_default()
    };
    let (shots, shots_on_goal, _) = triple("Shots / On goal / Goals scored");
    let shots_by_period = ["1st period", "2nd period", "3rd period", "Overtime"]
        .iter()
        .filter_map(|label| find(entries, shots_group, label).and_then(|c| c.triple()))
        .collect();
    let count_in = |group: &str, label: &str| {
        find(entries, group, label)
            .and_then(|c| c.count())
            .unwrap_or_default()
    };
    let faceoffs_won_by_zone = [
        count_in("Faceoffs", "Defensive zone"),
        count_in("Faceoffs", "Neutral zone"),
        count_in("Faceoffs", "Offensive zone"),
    ];
    let (power_plays, power_play_goals) = find_any(entries, "Power play / with the goal scored")
        .and_then(|c| c.ratio())
        .unwrap_or_default();
    let clock = |label: &str| {
        Seconds(f64::from(
            find_any(entries, label)
                .and_then(|c| c.seconds())
                .unwrap_or_default(),
        ))
    };
    let possession_group = "Puck possessions at even";
    let possession_pct_by_period = ["1st period", "2nd period", "3rd period"]
        .iter()
        .filter_map(|label| find(entries, possession_group, label).and_then(|c| c.percent()))
        .collect();
    TeamSummary {
        shots,
        shots_on_goal,
        shots_by_period,
        even_strength_shots: triple("At even strength"),
        power_play_shots: triple("Power play"),
        scoring_chances: triple("Shots from a scoring chance area"),
        xg: find(entries, shots_group, "xG").and_then(|c| c.decimal()),
        blocked_shots: count_in(shots_group, "Blocked shots"),
        faceoffs_won: count_in("Faceoffs", "Faceoffs won"),
        faceoffs_won_by_zone,
        puck_battles_won: count_in("Challenges", "Challenges won"),
        penalties: find_any(entries, "Penalties")
            .and_then(|c| c.count())
            .unwrap_or_default(),
        penalty_time: clock("Penalty time"),
        power_plays,
        power_play_goals,
        power_play_time: clock("Power play minutes played"),
        possession_time: Seconds(f64::from(
            find(entries, possession_group, "Puck possessions")
                .and_then(|c| c.seconds())
                .unwrap_or_default(),
        )),
        possession_pct: find(entries, possession_group, "Puck possessions, %")
            .and_then(|c| c.percent()),
        possession_pct_by_period,
        hits: find_any(entries, "Hits")
            .and_then(|c| c.count())
            .unwrap_or_default(),
    }
}
