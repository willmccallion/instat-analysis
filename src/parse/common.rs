//! Pieces shared by the page parsers: row labels, header lines, generic player tables.

use crate::error::Error;
use crate::layout::{self, Column, Line};
use crate::model::{CellValue, StatEntry, TeamName};
use crate::pdf::{Page, Word};

/// Line-grouping tolerance for table rows, in points.
pub const LINE_TOLERANCE: f64 = 1.5;
/// Words closer than this (points) belong to the same header phrase.
pub const PHRASE_GAP: f64 = 2.0;
/// How far left of a column's header a value may start.
pub const COLUMN_SLACK: f64 = 2.0;
pub const PAGE_RIGHT: f64 = 842.0;

/// A player as a match-report row labels them: number and surname.
///
/// The number in the match report's per-player tables is unreliable (InStat prints wrong
/// jerseys there), so it is only used to compare row orders between tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowLabel {
    /// Missing when InStat omits it (seen on some time-distribution rows).
    pub number: Option<u16>,
    pub surname: String,
}

impl RowLabel {
    /// Parses `"18 Taylor"`; returns `None` for anything else (e.g. `"TOTAL"`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (number, surname) = text.trim().split_once(' ')?;
        let number = number.parse().ok()?;
        let surname = surname.trim();
        surname
            .starts_with(|c: char| c.is_alphabetic())
            .then(|| Self {
                number: Some(number),
                surname: surname.to_owned(),
            })
    }

    /// Builds a label from an optional number word and surname words.
    #[must_use]
    pub fn from_parts(number: Option<&str>, surname: &str) -> Option<Self> {
        let surname = surname.trim();
        if !surname.starts_with(|c: char| c.is_alphabetic()) {
            return None;
        }
        Some(Self {
            number: number.and_then(|n| n.parse().ok()),
            surname: surname.to_owned(),
        })
    }
}

/// Splits `"21 Brooks 18 Taylor"` into labelled players.
#[must_use]
pub fn parse_members(text: &str) -> Vec<RowLabel> {
    let mut members: Vec<RowLabel> = Vec::new();
    let mut current: Option<RowLabel> = None;
    for token in text.split_whitespace() {
        if let Ok(number) = token.parse::<u16>() {
            members.extend(current.take());
            current = Some(RowLabel {
                number: Some(number),
                surname: String::new(),
            });
        } else if let Some(label) = current.as_mut() {
            if !label.surname.is_empty() {
                label.surname.push(' ');
            }
            label.surname.push_str(token);
        }
    }
    members.extend(current);
    members.retain(|m| !m.surname.is_empty());
    members
}

/// The report-wide header line (`"01.10.2026. SSAC LIONS U15 AAA 3:2 NORTH STARS …"`) and the page
/// title below it (`"PLAYERS' STATS: SSAC LIONS U15 AAA 3"`).
#[derive(Debug, Clone)]
pub struct PageHeading {
    pub title: String,
    /// Team named after the colon, if any.
    pub team: Option<TeamName>,
}

const HEADING_MAX_Y: f64 = 30.0;

#[must_use]
pub fn page_heading(page: &Page) -> Option<PageHeading> {
    let top_lines = layout::lines(&page.words, LINE_TOLERANCE);
    let heading = top_lines
        .iter()
        .filter(|l| l.y < HEADING_MAX_Y)
        .nth(1)?;
    let mut words: Vec<&str> = heading.words.iter().map(|w| w.text.as_str()).collect();
    if words.last().is_some_and(|w| w.parse::<u32>().is_ok()) {
        words.pop();
    }
    let text = words.join(" ");
    let (title, team) = match text.split_once(':') {
        Some((title, team)) => (title.trim().to_owned(), Some(TeamName::new(team))),
        None => (text.trim().to_owned(), None),
    };
    Some(PageHeading { title, team })
}

/// One row of a generic per-player table.
#[derive(Debug, Clone)]
pub struct PlayerRow {
    pub label: RowLabel,
    pub cells: Vec<(String, String)>,
}

impl PlayerRow {
    /// Raw text of the column whose header starts with `prefix`.
    #[must_use]
    pub fn get(&self, prefix: &str) -> Option<&str> {
        self.cells
            .iter()
            .find(|(name, _)| name.starts_with(prefix))
            .map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn get_exact(&self, name: &str) -> Option<&str> {
        self.cells
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn entries(&self, group: &str) -> Vec<StatEntry> {
        self.cells
            .iter()
            .map(|(label, value)| StatEntry {
                group: group.to_owned(),
                label: label.clone(),
                value: CellValue::from_text(value),
            })
            .collect()
    }
}

/// A data row starts with `"<number> <surname>"`, or just the surname when InStat omits
/// the number (the surname is then indented where it would follow the number).
fn is_label_start(line: &Line<'_>, label_x: f64) -> bool {
    let mut words = line.words.iter();
    let (Some(first), Some(second)) = (words.next(), words.next()) else {
        return false;
    };
    let is_alpha = |w: &Word| w.text.starts_with(|c: char| c.is_alphabetic()) && w.text != "TOTAL";
    let numbered = (first.x0 - label_x).abs() < 3.0 && first.text.parse::<u16>().is_ok() && is_alpha(second);
    let unnumbered = first.x0 > label_x + 3.0 && first.x0 < label_x + 20.0 && is_alpha(first);
    numbered || unnumbered
}

/// Parses a table of per-player rows found in the x-window `[x0, x1)` below `title_y`.
///
/// Header lines are those between the title and the first data row; data rows start with
/// `"<number> <surname>"` at the window's left edge. Stops at the first non-data line after
/// data has begun (e.g. a `TOTAL` row or the next section).
pub fn player_table(
    words: &[Word],
    x0: f64,
    x1: f64,
    title_y: f64,
    section: &'static str,
) -> Result<Vec<PlayerRow>, Error> {
    let region = layout::words_in(words, x0, x1, title_y + 1.0, f64::INFINITY);
    let lines = layout::lines(&region, LINE_TOLERANCE);
    let label_x = region
        .iter()
        .filter(|w| w.text.parse::<u16>().is_ok())
        .map(|w| w.x0)
        .fold(f64::INFINITY, f64::min);
    let first_data = lines
        .iter()
        .position(|l| is_label_start(l, label_x))
        .ok_or_else(|| Error::parse(section, "no player rows found"))?;
    let header_lines: Vec<Line<'_>> = lines[..first_data]
        .iter()
        .map(|l| Line {
            y: l.y,
            words: l.words.iter().copied().filter(|w| w.x0 > label_x + 30.0).collect(),
        })
        .filter(|l| !l.words.is_empty())
        .collect();
    let columns = layout::header_columns(&header_lines, PHRASE_GAP);
    let first_column_x = columns
        .first()
        .map(|c| c.x)
        .ok_or_else(|| Error::parse(section, "no header columns"))?;
    let mut rows = Vec::new();
    for line in &lines[first_data..] {
        if !is_label_start(line, label_x) {
            break;
        }
        let label_text = line.text_between(label_x - 1.0, first_column_x - COLUMN_SLACK);
        let label = RowLabel::parse(&label_text)
            .or_else(|| RowLabel::from_parts(None, &label_text))
            .ok_or_else(|| Error::parse(section, format!("bad row label {label_text:?}")))?;
        let values = layout::cells(line, &columns, COLUMN_SLACK, x1);
        let cells = columns
            .iter()
            .map(|c: &Column| c.name.clone())
            .zip(values)
            .collect();
        rows.push(PlayerRow { label, cells });
    }
    Ok(rows)
}

/// Finds the first line containing `phrase`, returning its y and the phrase's x.
#[must_use]
pub fn find_phrase(lines: &[Line<'_>], phrase: &str) -> Option<(f64, f64)> {
    lines
        .iter()
        .find_map(|l| l.find_phrase(phrase).map(|x| (l.y, x)))
}

pub fn require_phrase(
    lines: &[Line<'_>],
    phrase: &str,
    section: &'static str,
) -> Result<(f64, f64), Error> {
    find_phrase(lines, phrase)
        .ok_or_else(|| Error::parse(section, format!("missing {phrase:?}")))
}

#[cfg(test)]
mod tests {
    use super::{RowLabel, parse_members};

    #[test]
    fn row_label_needs_number_and_name() {
        assert_eq!(
            RowLabel::parse("18 Taylor"),
            Some(RowLabel {
                number: Some(18),
                surname: "Taylor".into()
            })
        );
        assert_eq!(RowLabel::parse("TOTAL"), None);
    }

    #[test]
    fn members_split_on_numbers_and_keep_multiword_surnames() {
        let members = parse_members("21 Brooks 18 Van Horn");
        assert_eq!(members.len(), 2);
        assert_eq!(members[1].surname, "Van Horn");
        assert_eq!(members[1].number, Some(18));
    }
}
