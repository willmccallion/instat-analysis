//! Counts printed inside the net and rink diagrams of player-report pages, placed by where
//! they sit in the drawing.

use crate::cell::Cell;
use crate::layout;
use crate::model::{NetArea, ShotZone};
use crate::parse::common::LINE_TOLERANCE;
use crate::pdf::{Rect, Word};

/// Words closer than this (points) make up one printed count, e.g. `"14"`, `"/"`, `"14"`.
const COUNT_GAP: f64 = 4.0;

/// A pair of counts (`a / b`, `a—b` or "—") printed in a diagram, at its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Printed {
    x: f64,
    y: f64,
    counts: (u32, u32),
}

/// Every `a / b` count in `region`; percentages and labels are skipped.
fn printed_counts(words: &[Word], region: &Rect) -> Vec<Printed> {
    printed(words, region, Cell::ratio)
}

/// Every pair `read` accepts in `region`, from words a point or two apart (InStat sometimes
/// splits `"7—13"` into `"7"` and `"—13"`).
fn printed(words: &[Word], region: &Rect, read: fn(&Cell) -> Option<(u32, u32)>) -> Vec<Printed> {
    let inside: Vec<Word> = words
        .iter()
        .filter(|w| region.contains(w.center_x(), w.center_y()))
        .cloned()
        .collect();
    let mut result = Vec::new();
    for line in layout::lines(&inside, LINE_TOLERANCE) {
        let mut groups: Vec<Vec<&Word>> = Vec::new();
        for word in &line.words {
            match groups.last_mut() {
                Some(group) if group.last().is_some_and(|last| word.x0 - last.x1 <= COUNT_GAP) => group.push(word),
                _ => groups.push(vec![word]),
            }
        }
        for group in groups {
            let text: Vec<&str> = group.iter().map(|w| w.text.as_str()).collect();
            let (Some(first), Some(last)) = (group.first(), group.last()) else {
                continue;
            };
            if let Some(counts) = read(&Cell::parse(&text.join(" "))) {
                result.push(Printed {
                    x: f64::midpoint(first.x0, last.x1),
                    y: line.y,
                    counts,
                });
            }
        }
    }
    result
}

fn sort_by_x(values: &mut [Printed]) {
    values.sort_by(|a, b| a.x.total_cmp(&b.x));
}

fn sort_by_y(values: &mut [Printed]) {
    values.sort_by(|a, b| a.y.total_cmp(&b.y));
}

/// The nine counts of a net diagram, or `None` unless exactly nine were printed.
#[must_use]
pub fn net_grid(words: &[Word], region: &Rect) -> Option<Vec<(NetArea, (u32, u32))>> {
    let mut values = printed_counts(words, region);
    if values.len() != 9 {
        return None;
    }
    sort_by_y(&mut values);
    let mut result = Vec::new();
    for (row, areas) in values.chunks_mut(3).zip(NetArea::GRID) {
        sort_by_x(row);
        result.extend(areas.into_iter().zip(row.iter().map(|p| p.counts)));
    }
    Some(result)
}

/// The seven counts of InStat's half-rink zone diagram (net at the top), or `None` unless
/// exactly seven were printed.
///
/// The three lowest counts are the blue-line zones; of the other four the outermost are the
/// flanks and the higher of the middle two is the slot.
#[must_use]
pub fn zone_grid(words: &[Word], region: &Rect) -> Option<Vec<(ShotZone, (u32, u32))>> {
    let mut values = printed_counts(words, region);
    if values.len() != 7 {
        return None;
    }
    sort_by_y(&mut values);
    let (upper, blue_line) = values.split_at_mut(4);
    sort_by_x(blue_line);
    sort_by_x(upper);
    let (left, middle, right) = (upper[0], [upper[1], upper[2]], upper[3]);
    let (slot, center) = if middle[0].y <= middle[1].y {
        (middle[0], middle[1])
    } else {
        (middle[1], middle[0])
    };
    let mut result = vec![
        (ShotZone::Slot, slot.counts),
        (ShotZone::Center, center.counts),
        (ShotZone::LeftFlank, left.counts),
        (ShotZone::RightFlank, right.counts),
    ];
    let blue_line_zones = [ShotZone::BlueLineLeft, ShotZone::BlueLineCenter, ShotZone::BlueLineRight];
    result.extend(blue_line_zones.into_iter().zip(blue_line.iter().map(|p| p.counts)));
    result.sort_by_key(|(zone, _)| *zone);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{net_grid, zone_grid};
    use crate::model::{NetArea, ShotZone};
    use crate::pdf::{Rect, Word};

    fn word(text: &str, x0: f64, y: f64) -> Word {
        Word {
            text: text.to_owned(),
            x0,
            x1: x0 + 4.0 * text.chars().count() as f64,
            top: y - 3.0,
            bottom: y + 3.0,
        }
    }

    /// `"a / b"` as InStat prints it: three words a point or two apart.
    fn count(a: u32, b: u32, x: f64, y: f64) -> Vec<Word> {
        let a = a.to_string();
        let slash_x = x + 4.0 * a.chars().count() as f64 + 1.5;
        vec![word(&a, x, y), word("/", slash_x, y), word(&b.to_string(), slash_x + 5.5, y)]
    }

    const EVERYWHERE: Rect = Rect {
        x0: 0.0,
        x1: 1000.0,
        top: 0.0,
        bottom: 1000.0,
    };

    #[test]
    fn net_counts_fill_rows_top_to_bottom_and_skip_percentages() {
        let mut words = Vec::new();
        for (row, y) in [100.0, 140.0, 180.0].into_iter().enumerate() {
            for (column, x) in [50.0, 110.0, 170.0].into_iter().enumerate() {
                let n = u32::try_from(row * 3 + column).unwrap();
                words.push(word("75%", x, y - 7.0));
                words.extend(count(n + 1, n, x, y + if column == 1 { -3.0 } else { 0.0 }));
            }
        }

        let grid = net_grid(&words, &EVERYWHERE).unwrap();

        assert_eq!(grid[0], (NetArea::TopLeft, (1, 0)));
        assert_eq!(grid[1], (NetArea::TopCenter, (2, 1)));
        assert_eq!(grid[8], (NetArea::BottomRight, (9, 8)));
    }

    #[test]
    fn a_dash_is_an_empty_net_area() {
        let mut words = vec![word("—", 110.0, 100.0)];
        for (x, y) in [(50.0, 100.0), (170.0, 100.0), (50.0, 140.0), (110.0, 140.0), (170.0, 140.0), (50.0, 180.0), (110.0, 180.0), (170.0, 180.0)] {
            words.extend(count(3, 2, x, y));
        }

        let grid = net_grid(&words, &EVERYWHERE).unwrap();

        assert_eq!(grid[1], (NetArea::TopCenter, (0, 0)));
    }

    #[test]
    fn missing_counts_give_no_grid() {
        let words = count(3, 2, 50.0, 100.0);

        assert_eq!(net_grid(&words, &EVERYWHERE), None);
    }

    #[test]
    fn zones_are_placed_like_the_diagram() {
        let mut words = Vec::new();
        words.extend(count(10, 6, 315.0, 471.0));
        words.extend(count(3, 3, 254.0, 495.0));
        words.extend(count(10, 9, 377.0, 495.0));
        words.extend(count(13, 13, 314.0, 508.0));
        words.extend(count(6, 6, 254.0, 544.0));
        words.extend(count(7, 7, 316.0, 544.0));
        words.extend(count(2, 1, 378.0, 544.0));

        let zones = zone_grid(&words, &EVERYWHERE).unwrap();

        assert_eq!(
            zones,
            [
                (ShotZone::Slot, (10, 6)),
                (ShotZone::Center, (13, 13)),
                (ShotZone::RightFlank, (10, 9)),
                (ShotZone::LeftFlank, (3, 3)),
                (ShotZone::BlueLineRight, (2, 1)),
                (ShotZone::BlueLineCenter, (7, 7)),
                (ShotZone::BlueLineLeft, (6, 6)),
            ]
        );
    }
}
