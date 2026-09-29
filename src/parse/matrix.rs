//! Player-by-player matrices such as the PASSES DISTRIBUTION page.
//!
//! Each row is printed as three lines: number, values, surname. Rows and columns are
//! anchored on surnames because InStat occasionally omits a number.

use crate::cell::Cell;
use crate::error::Error;
use crate::parse::common::RowLabel;
use crate::pdf::{Page, Word};

const SECTION: &str = "distribution matrix";
const LABEL_MAX_X: f64 = 90.0;

#[derive(Debug, Clone)]
pub struct RawMatrix {
    pub columns: Vec<RowLabel>,
    pub rows: Vec<(RowLabel, Vec<Cell>)>,
}

fn is_surname(word: &Word) -> bool {
    word.text.starts_with(|c: char| c.is_alphabetic()) && word.text != "TOTAL"
}

fn number_above<'a>(words: &'a [Word], surname: &Word, max_dx: f64) -> Option<&'a Word> {
    words
        .iter()
        .filter(|w| w.text.parse::<u16>().is_ok())
        .filter(|w| {
            let dy = surname.center_y() - w.center_y();
            dy > 2.0 && dy < 12.0 && (w.x0 - surname.x0).abs() < max_dx
        })
        .min_by(|a, b| {
            (surname.center_y() - a.center_y()).total_cmp(&(surname.center_y() - b.center_y()))
        })
}

/// Surname words at `x` on one line; multi-word surnames are joined.
fn group_surnames(mut words: Vec<&Word>, same_row_dx: f64) -> Vec<(Word, String)> {
    words.sort_by(|a, b| {
        a.center_y()
            .total_cmp(&b.center_y())
            .then(a.x0.total_cmp(&b.x0))
    });
    let mut result: Vec<(Word, String)> = Vec::new();
    for word in words {
        match result.last_mut() {
            Some((first, text))
                if (word.center_y() - first.center_y()).abs() < 1.5
                    && (0.0..same_row_dx).contains(&(word.x0 - first.x1)) =>
            {
                text.push(' ');
                text.push_str(&word.text);
                first.x1 = word.x1;
            }
            _ => result.push((word.clone(), word.text.clone())),
        }
    }
    result
}

/// Values are centred under their column's surname, and wide ones (`"1—1"`) start left of it.
fn nearest_column(centers: &[f64], x: f64) -> Option<usize> {
    centers
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|(index, _)| index)
}

pub fn parse(page: &Page) -> Result<RawMatrix, Error> {
    let words = &page.words;
    let total_header = words
        .iter()
        .filter(|w| w.text == "TOTAL" && w.x0 > LABEL_MAX_X)
        .min_by(|a, b| a.top.total_cmp(&b.top))
        .ok_or_else(|| Error::parse(SECTION, "no TOTAL column"))?;
    let header_y = total_header.center_y();

    // Only column headers sit on this band; the first can start left of LABEL_MAX_X.
    let header_surnames: Vec<&Word> = words
        .iter()
        .filter(|w| is_surname(w) && w.x0 < total_header.x0)
        .filter(|w| w.center_y() > header_y && w.center_y() < header_y + 10.0)
        .collect();
    let mut column_anchors = group_surnames(header_surnames, 2.0);
    column_anchors.sort_by(|a, b| a.0.x0.total_cmp(&b.0.x0));
    let columns: Vec<RowLabel> = column_anchors
        .iter()
        .map(|(anchor, surname)| {
            let number = number_above(words, anchor, 3.0).map(|w| w.text.as_str());
            RowLabel::from_parts(number, surname)
                .ok_or_else(|| Error::parse(SECTION, format!("bad column label {surname:?}")))
        })
        .collect::<Result<_, _>>()?;
    let column_centers: Vec<f64> = column_anchors.iter().map(|(w, _)| w.center_x()).collect();

    let row_surnames: Vec<&Word> = words
        .iter()
        .filter(|w| is_surname(w) && w.x0 < LABEL_MAX_X && w.center_y() > header_y + 10.0)
        .collect();
    let row_anchors = group_surnames(row_surnames, 2.0);
    let labels_right = row_anchors.iter().map(|(w, _)| w.x1).fold(f64::NEG_INFINITY, f64::max);
    let rows = row_anchors
        .iter()
        .map(|(anchor, surname)| {
            let number = number_above(words, anchor, 5.0).map(|w| w.text.as_str());
            let label = RowLabel::from_parts(number, surname)
                .ok_or_else(|| Error::parse(SECTION, format!("bad row label {surname:?}")))?;
            let mut cells = vec![String::new(); columns.len()];
            for word in words.iter().filter(|w| {
                let dy = anchor.center_y() - w.center_y();
                w.x0 > labels_right && w.x0 < total_header.x0 - 2.0 && dy > 0.5 && dy < 8.0
            }) {
                if let Some(index) = nearest_column(&column_centers, word.center_x()) {
                    if !cells[index].is_empty() {
                        cells[index].push(' ');
                    }
                    cells[index].push_str(&word.text);
                }
            }
            Ok((label, cells.iter().map(|c| Cell::parse(c)).collect()))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if rows.is_empty() {
        return Err(Error::parse(SECTION, "no rows"));
    }
    Ok(RawMatrix { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::cell::Cell;
    use crate::pdf::{Page, Word};

    fn word(text: &str, x0: f64, x1: f64, top: f64) -> Word {
        Word {
            text: text.to_owned(),
            x0,
            x1,
            top,
            bottom: top + 6.5,
        }
    }

    #[test]
    fn first_column_and_wide_centred_cells_are_kept() {
        let page = Page {
            number: 1,
            words: vec![
                word("TOTAL", 200.0, 216.0, 92.7),
                word("5", 85.4, 90.2, 90.3),
                word("Ashby", 85.4, 100.7, 98.1),
                word("Baker", 118.5, 138.7, 98.1),
                word("18", 32.2, 41.8, 109.9),
                word("1—1", 85.9, 99.9, 113.1),
                word("—", 126.4, 130.7, 113.1),
                word("Taylor", 32.2, 54.2, 117.7),
            ],
            fills: Vec::new(),
            clips: Vec::new(),
            shapes: Vec::new(),
        };

        let matrix = parse(&page).unwrap();

        let columns: Vec<(Option<u16>, &str)> = matrix.columns.iter().map(|c| (c.number, c.surname.as_str())).collect();
        assert_eq!(columns, [(Some(5), "Ashby"), (None, "Baker")]);
        assert_eq!(matrix.rows.len(), 1);
        assert_eq!(matrix.rows[0].0.surname, "Taylor");
        assert_eq!(matrix.rows[0].1, [Cell::Pair(1, 1), Cell::Empty]);
    }
}
