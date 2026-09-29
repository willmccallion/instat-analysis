//! Geometry helpers for turning positioned words into lines, columns and cells.

use crate::pdf::Word;

/// Words sharing a baseline, sorted left to right.
#[derive(Debug, Clone)]
pub struct Line<'a> {
    pub y: f64,
    pub words: Vec<&'a Word>,
}

impl Line<'_> {
    /// Space-joined text of words whose left edge lies in `[x0, x1)`.
    #[must_use]
    pub fn text_between(&self, x0: f64, x1: f64) -> String {
        join(self.words.iter().copied().filter(|w| w.x0 >= x0 && w.x0 < x1))
    }

    #[must_use]
    pub fn text(&self) -> String {
        join(self.words.iter().copied())
    }

    #[must_use]
    pub fn first_x(&self) -> f64 {
        self.words.first().map_or(f64::INFINITY, |w| w.x0)
    }

    /// Whether the line contains `needle` as a run of consecutive words.
    #[must_use]
    pub fn contains_phrase(&self, needle: &str) -> bool {
        self.find_phrase(needle).is_some()
    }

    /// Left x of the first occurrence of `needle` as consecutive words.
    #[must_use]
    pub fn find_phrase(&self, needle: &str) -> Option<f64> {
        let parts: Vec<&str> = needle.split_whitespace().collect();
        if parts.is_empty() || parts.len() > self.words.len() {
            return None;
        }
        self.words
            .windows(parts.len())
            .find(|window| window.iter().zip(&parts).all(|(w, p)| w.text == *p))
            .map(|window| window[0].x0)
    }
}

fn join<'a>(words: impl Iterator<Item = &'a Word>) -> String {
    let mut out = String::new();
    for word in words {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word.text);
    }
    out
}

/// Groups words into lines by vertical centre, `tolerance` points apart at most.
#[must_use]
pub fn lines(words: &[Word], tolerance: f64) -> Vec<Line<'_>> {
    let mut sorted: Vec<&Word> = words.iter().collect();
    sorted.sort_by(|a, b| a.center_y().total_cmp(&b.center_y()));
    let mut result: Vec<Line<'_>> = Vec::new();
    for word in sorted {
        match result.last_mut() {
            Some(line) if (word.center_y() - line.y).abs() <= tolerance => line.words.push(word),
            _ => result.push(Line {
                y: word.center_y(),
                words: vec![word],
            }),
        }
    }
    for line in &mut result {
        line.words.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }
    result
}

/// Words inside an x/y window.
#[must_use]
pub fn words_in(words: &[Word], x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<Word> {
    words
        .iter()
        .filter(|w| w.x0 >= x0 && w.x0 < x1 && w.center_y() >= y0 && w.center_y() < y1)
        .cloned()
        .collect()
}

/// A table column: where it starts and its (possibly multi-line) header text.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub x: f64,
    pub name: String,
}

/// Builds columns from header lines: words closer than `phrase_gap` form one phrase, and
/// phrases starting at (nearly) the same x on different lines stack into one header.
#[must_use]
pub fn header_columns(header_lines: &[Line<'_>], phrase_gap: f64) -> Vec<Column> {
    let mut columns: Vec<Column> = Vec::new();
    for line in header_lines {
        for (x, phrase) in phrases(line, phrase_gap) {
            match columns.iter_mut().find(|c| (c.x - x).abs() < 2.0) {
                Some(column) => {
                    column.name.push(' ');
                    column.name.push_str(&phrase);
                }
                None => columns.push(Column { x, name: phrase }),
            }
        }
    }
    columns.sort_by(|a, b| a.x.total_cmp(&b.x));
    columns
}

fn phrases(line: &Line<'_>, phrase_gap: f64) -> Vec<(f64, String)> {
    let mut result: Vec<(f64, String, f64)> = Vec::new();
    for word in &line.words {
        match result.last_mut() {
            Some((_, text, end)) if word.x0 - *end <= phrase_gap => {
                text.push(' ');
                text.push_str(&word.text);
                *end = word.x1;
            }
            _ => result.push((word.x0, word.text.clone(), word.x1)),
        }
    }
    result.into_iter().map(|(x, text, _)| (x, text)).collect()
}

/// Splits a line into cells, one per column; cell i holds words starting in
/// `[columns[i].x - slack, columns[i+1].x - slack)`.
#[must_use]
pub fn cells(line: &Line<'_>, columns: &[Column], slack: f64, right_edge: f64) -> Vec<String> {
    columns
        .iter()
        .enumerate()
        .map(|(i, column)| {
            let end = columns.get(i + 1).map_or(right_edge, |next| next.x - slack);
            line.text_between(column.x - slack, end)
        })
        .collect()
}

/// Index of the column whose span contains `x` (spans as in [`cells`]).
#[must_use]
pub fn column_index(columns: &[Column], x: f64, slack: f64) -> Option<usize> {
    columns
        .iter()
        .rposition(|column| x >= column.x - slack)
}
