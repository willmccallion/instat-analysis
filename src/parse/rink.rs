//! Markers on InStat's rink drawings, placed on a standard rink in feet.
//!
//! InStat's drawings are not to scale, so each is calibrated from its own faceoff markings:
//! end-zone faceoff spots sit 22 ft either side of the middle and 20 ft out from the goal
//! line, which is 89 ft from centre ice.

use crate::model::{Feet, Jersey, RinkPoint};
use crate::pdf::{Page, Rect, Rgb, Shape, Word};

const SPOT_ACROSS: f64 = 22.0;
const SPOT_OUT: f64 = 20.0;
const GOAL_LINE: f64 = 89.0;
const HALF_WIDTH: f64 = 42.5;
const BEHIND_GOAL_LINE: f64 = 11.0;
const GOAL_LINE_TO_BLUE_LINE: f64 = 64.0;

const FACEOFF_PINK: Rgb = Rgb { r: 0.98, g: 0.79, b: 0.79 };
const GOAL_RED: Rgb = Rgb { r: 1.0, g: 0.0, b: 0.0 };
/// End-zone faceoff circles on the half-rink charts are about 54 points across.
const CIRCLE_WIDTH: std::ops::RangeInclusive<f64> = 50.0..=58.0;
/// Shot markers are outlined circles about 8 points across.
const SHOT_MARKER_WIDTH: std::ops::RangeInclusive<f64> = 6.0..=10.0;

/// Page-to-rink mapping for one half-rink chart, net at the top.
#[derive(Debug, Clone, Copy, PartialEq)]
struct HalfRink {
    center_x: f64,
    spot_y: f64,
    points_per_foot: f64,
}

impl HalfRink {
    fn point(&self, x: f64, y: f64) -> RinkPoint {
        let out = (y - self.spot_y) / self.points_per_foot + SPOT_OUT;
        RinkPoint {
            along: Feet(GOAL_LINE - out),
            across: Feet((x - self.center_x) / self.points_per_foot),
        }
    }

    /// The drawing's extent on the page, from the end boards to the blue line.
    fn bounds(&self) -> Rect {
        let feet = |f: f64| f * self.points_per_foot;
        Rect {
            x0: self.center_x - feet(HALF_WIDTH),
            x1: self.center_x + feet(HALF_WIDTH),
            top: self.spot_y - feet(SPOT_OUT + BEHIND_GOAL_LINE),
            bottom: self.spot_y + feet(GOAL_LINE_TO_BLUE_LINE - SPOT_OUT),
        }
    }
}

/// One half-rink per pair of end-zone faceoff circles side by side.
fn half_rinks(shapes: &[Shape]) -> Vec<HalfRink> {
    let mut circles: Vec<&Rect> = shapes
        .iter()
        .filter(|s| !s.stroked && s.fill.near(&FACEOFF_PINK))
        .map(|s| &s.bounds)
        .filter(|r| CIRCLE_WIDTH.contains(&r.width()) && (r.width() - r.height()).abs() < 1.0)
        .collect();
    circles.sort_by(|a, b| a.center_y().total_cmp(&b.center_y()).then(a.x0.total_cmp(&b.x0)));
    circles
        .windows(2)
        .filter_map(|pair| {
            let (left, right) = (pair[0], pair[1]);
            let spacing = right.center_x() - left.center_x();
            let side_by_side = (left.center_y() - right.center_y()).abs() < 1.0 && spacing > left.width() && spacing < 3.0 * left.width();
            side_by_side.then(|| HalfRink {
                center_x: f64::midpoint(left.center_x(), right.center_x()),
                spot_y: f64::midpoint(left.center_y(), right.center_y()),
                points_per_foot: spacing / (2.0 * SPOT_ACROSS),
            })
        })
        .collect()
}

/// `"1ST PERIOD"` → 1, `"OVERTIME"` → 4, with the label's centre.
fn period_labels(words: &[Word]) -> Vec<(u32, f64, f64)> {
    let mut labels = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if word.text == "OVERTIME" {
            labels.push((4, word.center_x(), word.center_y()));
        } else if word.text == "PERIOD"
            && let Some(ordinal) = i.checked_sub(1).and_then(|p| words.get(p))
            && let Ok(period) = ordinal.text.trim_end_matches(char::is_alphabetic).parse::<u32>()
        {
            labels.push((period, f64::midpoint(ordinal.x0, word.x1), word.center_y()));
        }
    }
    labels
}

/// A shot marker as the shooting chart draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawShot {
    pub period: u32,
    /// The number printed in the marker; the chart uses real jersey numbers.
    pub jersey: Option<Jersey>,
    pub at: RinkPoint,
    pub goal: bool,
}

/// Markers overlap, so a neighbour's number can sit inside this marker; take the most central.
fn marker_number(words: &[Word], marker: &Rect) -> Option<Jersey> {
    let offset = |w: &Word| (w.center_x() - marker.center_x()).hypot(w.center_y() - marker.center_y());
    words
        .iter()
        .filter(|w| marker.contains(w.center_x(), w.center_y()))
        .filter_map(|w| Some((offset(w), w.text.parse().ok()?)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, number)| Jersey(number))
}

/// Every shot on the per-period half-rinks of a SHOTS page. Charts without a period label
/// (the all-game summary) are skipped.
#[must_use]
pub fn shooting_chart(page: &Page) -> Vec<RawShot> {
    let labels = period_labels(&page.words);
    let mut shots = Vec::new();
    for rink in half_rinks(&page.shapes) {
        let bounds = rink.bounds();
        let Some(period) = labels
            .iter()
            .filter(|(_, x, y)| bounds.x0 < *x && *x < bounds.x1 && *y < bounds.top)
            .min_by(|a, b| (bounds.top - a.2).total_cmp(&(bounds.top - b.2)))
            .map(|(period, _, _)| *period)
        else {
            continue;
        };
        let markers = page.shapes.iter().filter(|s| {
            s.stroked
                && SHOT_MARKER_WIDTH.contains(&s.bounds.width())
                && SHOT_MARKER_WIDTH.contains(&s.bounds.height())
                && bounds.contains(s.bounds.center_x(), s.bounds.center_y())
        });
        for marker in markers {
            shots.push(RawShot {
                period,
                jersey: marker_number(&page.words, &marker.bounds),
                at: rink.point(marker.bounds.center_x(), marker.bounds.center_y()),
                goal: marker.fill.near(&GOAL_RED),
            });
        }
    }
    shots
}

#[cfg(test)]
mod tests {
    use super::shooting_chart;
    use crate::model::Jersey;
    use crate::pdf::{Page, Rect, Rgb, Shape, Word};

    fn word(text: &str, x0: f64, x1: f64, top: f64) -> Word {
        Word {
            text: text.to_owned(),
            x0,
            x1,
            top,
            bottom: top + 6.0,
        }
    }

    fn shape(x0: f64, top: f64, size: f64, fill: Rgb, stroked: bool) -> Shape {
        Shape {
            bounds: Rect {
                x0,
                x1: x0 + size,
                top,
                bottom: top + size,
            },
            fill,
            stroked,
        }
    }

    const PINK: Rgb = Rgb { r: 0.98, g: 0.79, b: 0.79 };
    const GREY: Rgb = Rgb { r: 0.88, g: 0.88, b: 0.88 };
    const RED: Rgb = Rgb { r: 1.0, g: 0.0, b: 0.0 };

    /// A half-rink whose faceoff spots are 88 points apart (2 points per foot) at y = 400.
    fn page(markers: Vec<Shape>, mut words: Vec<Word>) -> Page {
        let mut shapes = vec![shape(129.0, 373.0, 54.0, PINK, false), shape(217.0, 373.0, 54.0, PINK, false)];
        shapes.extend(markers);
        words.extend([word("2ND", 180.0, 192.0, 320.0), word("PERIOD", 194.0, 214.0, 320.0)]);
        Page {
            number: 6,
            words,
            fills: Vec::new(),
            clips: Vec::new(),
            shapes,
        }
    }

    #[test]
    fn markers_are_placed_in_feet_from_the_faceoff_spots() {
        let marker = shape(216.0, 356.0, 8.0, GREY, true);

        let shots = shooting_chart(&page(vec![marker], vec![word("17", 217.0, 223.0, 357.0)]));

        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].period, 2);
        assert_eq!(shots[0].jersey, Some(Jersey(17)));
        assert!(!shots[0].goal);
        assert!((shots[0].at.across.0 - 10.0).abs() < 1e-9);
        assert!((shots[0].at.along.0 - 89.0).abs() < 1e-9, "on the goal line");
    }

    #[test]
    fn overlapping_markers_take_their_own_number() {
        let under = shape(210.0, 356.0, 8.0, GREY, true);
        let over = shape(215.0, 356.0, 8.0, GREY, true);
        let words = vec![word("12", 211.0, 217.0, 357.0), word("19", 216.0, 222.0, 357.0)];

        let shots = shooting_chart(&page(vec![under, over], words));

        let numbers: Vec<Option<Jersey>> = shots.iter().map(|s| s.jersey).collect();
        assert_eq!(numbers, [Some(Jersey(12)), Some(Jersey(19))]);
    }

    #[test]
    fn red_markers_are_goals() {
        let shots = shooting_chart(&page(vec![shape(196.0, 396.0, 8.0, RED, true)], Vec::new()));

        assert!(shots[0].goal);
        assert!(shots[0].at.across.0.abs() < 1e-9);
        assert!((shots[0].at.along.0 - 69.0).abs() < 1e-9);
    }

    #[test]
    fn charts_without_a_period_label_are_skipped() {
        let mut unlabelled = page(vec![shape(196.0, 396.0, 8.0, GREY, true)], Vec::new());
        unlabelled.words.clear();

        assert!(shooting_chart(&unlabelled).is_empty());
    }
}
