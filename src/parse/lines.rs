//! The match report's LINES STATS page, read only for who InStat lists among its defence
//! pairs and its forward lines. Unlike the report's per-player tables, this page prints
//! correct jersey numbers.

use std::collections::HashMap;

use crate::cell::Cell;
use crate::error::Error;
use crate::layout::{self, Line};
use crate::model::{Seconds, SkaterPosition};
use crate::parse::common::{COLUMN_SLACK, LINE_TOLERANCE, PAGE_RIGHT, PHRASE_GAP, RowLabel, find_phrase, parse_members};
use crate::pdf::{Page, Word};

const SECTION: &str = "lines stats";
/// The page's right half (power play, penalty kill and five-man units) starts here.
const RIGHT_HALF_HEADING: &str = "Power players lines";
const SECTIONS: [(&str, SkaterPosition); 2] = [("Defencemen lines", SkaterPosition::Defence), ("Forwards lines", SkaterPosition::Forward)];

/// A player in the forward lines or defence pairs, with the ice time of every unit they are
/// listed in there.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedPlayer {
    pub label: RowLabel,
    pub position: SkaterPosition,
    pub toi: Seconds,
}

pub fn parse(page: &Page) -> Result<Vec<ListedPlayer>, Error> {
    let all_lines = layout::lines(&page.words, LINE_TOLERANCE);
    let right_half = find_phrase(&all_lines, RIGHT_HALF_HEADING).map_or(PAGE_RIGHT, |(_, x)| x - 1.0);
    let left = layout::words_in(&page.words, 0.0, right_half, 0.0, f64::INFINITY);
    let lines = layout::lines(&left, LINE_TOLERANCE);
    let headings: Vec<(f64, SkaterPosition)> = SECTIONS.iter().filter_map(|(heading, position)| find_phrase(&lines, heading).map(|(y, _)| (y, *position))).collect();
    let mut toi: HashMap<(RowLabel, SkaterPosition), f64> = HashMap::new();
    for (start_y, position) in &headings {
        let end_y = headings.iter().map(|(y, _)| *y).filter(|y| y > start_y).fold(f64::INFINITY, f64::min);
        let section: Vec<&Line<'_>> = lines.iter().filter(|l| l.y > start_y + 0.5 && l.y < end_y - 0.5).collect();
        for (members, seconds) in units(&section)? {
            for label in members {
                *toi.entry((label, *position)).or_default() += seconds;
            }
        }
    }
    Ok(toi.into_iter().map(|((label, position), seconds)| ListedPlayer { label, position, toi: Seconds(seconds) }).collect())
}

/// Lines before the first unit: the column headers.
fn header_end(lines: &[&Line<'_>]) -> usize {
    lines
        .iter()
        .position(|l| {
            let has_clock = l.words.iter().any(|w| matches!(Cell::parse(&w.text), Cell::Clock(_)));
            let starts_with_player = l.words.len() >= 2 && l.words[0].text.parse::<u16>().is_ok() && l.words[1].text.starts_with(|c: char| c.is_alphabetic());
            has_clock || starts_with_player
        })
        .unwrap_or(lines.len())
}

/// Each unit's members and ice time.
fn units(lines: &[&Line<'_>]) -> Result<Vec<(Vec<RowLabel>, f64)>, Error> {
    let header_count = header_end(lines);
    let header_lines: Vec<Line<'_>> = lines[..header_count].iter().map(|l| (*l).clone()).collect();
    let columns = layout::header_columns(&header_lines, PHRASE_GAP);
    let toi_index = columns.iter().position(|c| c.name.starts_with("Time on ice")).ok_or_else(|| Error::parse(SECTION, "no 'Time on ice' column"))?;
    let toi_x = columns[toi_index].x;
    let is_stat = |w: &&Word| w.x0 >= toi_x - COLUMN_SLACK;
    lines[header_count..]
        .iter()
        .filter(|l| l.words.iter().any(&is_stat))
        .map(|line| {
            let members = parse_members(&line.text_between(0.0, toi_x - COLUMN_SLACK));
            if members.is_empty() {
                return Err(Error::parse(SECTION, format!("unit without players at y={:.1}", line.y)));
            }
            let toi_text = layout::cells(line, &columns, COLUMN_SLACK, PAGE_RIGHT)
                .into_iter()
                .nth(toi_index)
                .ok_or_else(|| Error::parse(SECTION, format!("no time on ice at y={:.1}", line.y)))?;
            let seconds = Cell::parse(&toi_text).seconds().ok_or_else(|| Error::parse(SECTION, format!("unreadable time on ice {toi_text:?}")))?;
            Ok((members, f64::from(seconds)))
        })
        .collect()
}
