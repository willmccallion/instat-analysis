//! The match report's TEAMS STATS page: four label/value blocks per section, one value column
//! per team. Only InStat's xG and possession are taken from it.

use crate::cell::Cell;
use crate::error::Error;
use crate::layout::{self, Line};
use crate::model::{CellValue, Seconds, StatEntry};
use crate::parse::common::{LINE_TOLERANCE, PAGE_RIGHT};
use crate::pdf::Page;

const SECTION: &str = "team stats";

/// All labelled values for both teams, in page order.
#[derive(Debug, Clone, Default)]
pub struct TeamStatsPage {
    pub abbreviations: [String; 2],
    pub entries: [Vec<StatEntry>; 2],
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
    Ok(result)
}

fn find<'a>(entries: &'a [StatEntry], group_prefix: &str, label: &str) -> Option<Cell> {
    entries
        .iter()
        .find(|e| e.group.starts_with(group_prefix) && e.label == label)
        .map(|e: &'a StatEntry| Cell::parse(&e.value.text))
}

/// What only InStat measures for a team: its xG and how long it had the puck.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstatTeamNumbers {
    pub xg: Option<f64>,
    pub possession_time: Seconds,
    pub possession_pct: Option<f64>,
    pub possession_pct_by_period: Vec<f64>,
}

#[must_use]
pub fn instat_numbers(entries: &[StatEntry]) -> InstatTeamNumbers {
    let possession_group = "Puck possessions at even";
    InstatTeamNumbers {
        xg: find(entries, "Shots and goals", "xG").and_then(|c| c.decimal()),
        possession_time: Seconds(f64::from(
            find(entries, possession_group, "Puck possessions").and_then(|c| c.seconds()).unwrap_or_default(),
        )),
        possession_pct: find(entries, possession_group, "Puck possessions, %").and_then(|c| c.percent()),
        possession_pct_by_period: ["1st period", "2nd period", "3rd period"]
            .iter()
            .filter_map(|label| find(entries, possession_group, label).and_then(|c| c.percent()))
            .collect(),
    }
}
