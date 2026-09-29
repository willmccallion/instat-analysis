//! The LINES STATS page: defence pairs and forward lines on the left; power-play units,
//! penalty-kill units and full five-man units on the right.

use crate::cell::Cell;
use crate::error::Error;
use crate::layout::{self, Column, Line};
use crate::model::{EvenStrengthUnitStats, Seconds, SpecialTeamsUnitStats, UnitKind, UnitStats};
use crate::parse::common::{
    COLUMN_SLACK, LINE_TOLERANCE, PAGE_RIGHT, PHRASE_GAP, RowLabel, find_phrase, parse_members,
};
use crate::pdf::{Page, Word};

const SECTION: &str = "lines stats";

/// A unit as printed: members carry correct jersey numbers on this page.
#[derive(Debug, Clone)]
pub struct RawUnit {
    pub kind: UnitKind,
    pub members: Vec<RowLabel>,
    pub toi: Seconds,
    pub stats: UnitStats,
}

struct SectionSpec {
    heading: &'static str,
    kind: UnitKind,
}

const LEFT_SECTIONS: [SectionSpec; 2] = [
    SectionSpec {
        heading: "Defencemen lines",
        kind: UnitKind::DefencePair,
    },
    SectionSpec {
        heading: "Forwards lines",
        kind: UnitKind::ForwardLine,
    },
];

const RIGHT_SECTIONS: [SectionSpec; 3] = [
    SectionSpec {
        heading: "Power players lines",
        kind: UnitKind::PowerPlay,
    },
    SectionSpec {
        heading: "Penalty killers lines",
        kind: UnitKind::PenaltyKill,
    },
    SectionSpec {
        heading: "Basic lines",
        kind: UnitKind::FullUnit,
    },
];

pub fn parse(page: &Page) -> Result<Vec<RawUnit>, Error> {
    let all_lines = layout::lines(&page.words, LINE_TOLERANCE);
    let (_, split_x) = find_phrase(&all_lines, "Power players lines")
        .ok_or_else(|| Error::parse(SECTION, "missing power play heading"))?;
    let left = layout::words_in(&page.words, 0.0, split_x - 1.0, 40.0, f64::INFINITY);
    let right = layout::words_in(&page.words, split_x - 1.0, PAGE_RIGHT, 40.0, f64::INFINITY);
    let mut units = Vec::new();
    units.extend(parse_half(&left, &LEFT_SECTIONS, false)?);
    units.extend(parse_half(&right, &RIGHT_SECTIONS, true)?);
    Ok(units)
}

fn parse_half(
    words: &[Word],
    specs: &[SectionSpec],
    names_on_own_lines: bool,
) -> Result<Vec<RawUnit>, Error> {
    let lines = layout::lines(words, LINE_TOLERANCE);
    let headings: Vec<(f64, &SectionSpec)> = specs
        .iter()
        .filter_map(|spec| find_phrase(&lines, spec.heading).map(|(y, _)| (y, spec)))
        .collect();
    let mut units = Vec::new();
    for (start_y, spec) in &headings {
        let end_y = headings
            .iter()
            .map(|(y, _)| *y)
            .filter(|y| y > start_y)
            .fold(f64::INFINITY, f64::min);
        let section_lines: Vec<&Line<'_>> = lines
            .iter()
            .filter(|l| l.y > *start_y + 0.5 && l.y < end_y - 0.5)
            .collect();
        units.extend(parse_section(
            &section_lines,
            spec.kind,
            names_on_own_lines,
        )?);
    }
    Ok(units)
}

fn is_stats_word(word: &Word, first_column_x: f64) -> bool {
    word.x0 >= first_column_x - COLUMN_SLACK
}

fn header_end(lines: &[&Line<'_>]) -> usize {
    lines
        .iter()
        .position(|l| {
            let has_clock = l
                .words
                .iter()
                .any(|w| matches!(Cell::parse(&w.text), Cell::Clock(_)));
            let starts_with_player = l.words.len() >= 2
                && l.words[0].text.parse::<u16>().is_ok()
                && l.words[1].text.starts_with(|c: char| c.is_alphabetic());
            has_clock || starts_with_player
        })
        .unwrap_or(lines.len())
}

fn parse_section(
    lines: &[&Line<'_>],
    kind: UnitKind,
    names_on_own_lines: bool,
) -> Result<Vec<RawUnit>, Error> {
    let header_count = header_end(lines);
    let header_lines: Vec<Line<'_>> = lines[..header_count].iter().map(|l| (*l).clone()).collect();
    let columns = layout::header_columns(&header_lines, PHRASE_GAP);
    let Some(first_column_x) = columns
        .iter()
        .find(|c| c.name.starts_with("Time on ice"))
        .map(|c| c.x)
    else {
        return Err(Error::parse(SECTION, "no 'Time on ice' column"));
    };
    let columns: Vec<Column> = columns
        .into_iter()
        .filter(|c| c.x >= first_column_x)
        .collect();
    let body = &lines[header_count..];
    let stat_lines: Vec<&Line<'_>> = body
        .iter()
        .copied()
        .filter(|l| l.words.iter().any(|w| is_stats_word(w, first_column_x)))
        .collect();
    let mut units = Vec::new();
    for stat_line in &stat_lines {
        let name_text = if names_on_own_lines {
            names_near(body, &stat_lines, stat_line, first_column_x)
        } else {
            stat_line.text_between(0.0, first_column_x - COLUMN_SLACK)
        };
        let members = parse_members(&name_text);
        if members.is_empty() {
            return Err(Error::parse(
                SECTION,
                format!("unit without players at y={:.1}", stat_line.y),
            ));
        }
        let values: Vec<(String, Cell)> = columns
            .iter()
            .map(|c| c.name.clone())
            .zip(layout::cells(stat_line, &columns, COLUMN_SLACK, PAGE_RIGHT))
            .map(|(name, text)| (name, Cell::parse(&text)))
            .collect();
        units.push(build_unit(kind, members, &values)?);
    }
    Ok(units)
}

/// Name-only lines belong to the stats line they are vertically closest to.
fn names_near(
    body: &[&Line<'_>],
    stat_lines: &[&Line<'_>],
    stat_line: &Line<'_>,
    first_column_x: f64,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in body {
        let names = line.text_between(0.0, first_column_x - COLUMN_SLACK);
        if names.is_empty() {
            continue;
        }
        let nearest = stat_lines
            .iter()
            .min_by(|a, b| (a.y - line.y).abs().total_cmp(&(b.y - line.y).abs()));
        if nearest.is_some_and(|n| (n.y - stat_line.y).abs() < f64::EPSILON) {
            parts.push(names);
        }
    }
    parts.join(" ")
}

fn value<'a>(values: &'a [(String, Cell)], prefix: &str) -> Option<&'a Cell> {
    values
        .iter()
        .find(|(name, _)| name.starts_with(prefix))
        .map(|(_, cell)| cell)
}

fn exact<'a>(values: &'a [(String, Cell)], name: &str) -> Option<&'a Cell> {
    values.iter().find(|(n, _)| n == name).map(|(_, cell)| cell)
}

fn required<T>(found: Option<T>, what: &str) -> Result<T, Error> {
    found.ok_or_else(|| Error::parse(SECTION, format!("unreadable {what}")))
}

fn build_unit(
    kind: UnitKind,
    members: Vec<RowLabel>,
    values: &[(String, Cell)],
) -> Result<RawUnit, Error> {
    let toi = Seconds(f64::from(required(
        value(values, "Time on ice").and_then(Cell::seconds),
        "time on ice",
    )?));
    let stats = match kind {
        UnitKind::DefencePair | UnitKind::ForwardLine | UnitKind::FullUnit => {
            let (goals_for, goals_against) =
                required(value(values, "Goals").and_then(Cell::pair), "goals")?;
            let (penalties_drawn, penalties_taken) =
                required(value(values, "Penalties").and_then(Cell::pair), "penalties")?;
            UnitStats::EvenStrength(EvenStrengthUnitStats {
                plus_minus: required(value(values, "+ / -").and_then(Cell::signed), "+/-")?,
                goals_for,
                goals_against,
                corsi_for: required(exact(values, "CORSI +").and_then(Cell::count), "CORSI+")?,
                corsi_against: required(exact(values, "CORSI -").and_then(Cell::count), "CORSI-")?,
                penalties_drawn,
                penalties_taken,
                possession_pct: value(values, "Possession").and_then(Cell::percent),
            })
        }
        UnitKind::PowerPlay | UnitKind::PenaltyKill => {
            let (shifts, goals) = required(
                value(values, "Shifts / goals").and_then(Cell::ratio),
                "shifts / goals",
            )?;
            let shots_cell = values
                .iter()
                .find(|(name, _)| {
                    name.contains("shots / on goal") || name.starts_with("Shots / on goal")
                })
                .map(|(_, cell)| cell);
            let (shots, shots_on_goal) = required(shots_cell.and_then(Cell::ratio), "shots")?;
            UnitStats::SpecialTeams(SpecialTeamsUnitStats {
                shifts,
                goals,
                shots,
                shots_on_goal,
                time_in_offensive_zone: Seconds(f64::from(
                    value(values, "Time on opp")
                        .and_then(Cell::seconds)
                        .unwrap_or_default(),
                )),
                faceoffs_won: value(values, "Faceoffs")
                    .and_then(Cell::count)
                    .unwrap_or_default(),
                opponent_breakouts: exact(values, "Opp breakouts")
                    .and_then(Cell::count)
                    .unwrap_or_default(),
            })
        }
    };
    Ok(RawUnit {
        kind,
        members,
        toi,
        stats,
    })
}
