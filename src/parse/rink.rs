//! Markers on InStat's rink drawings, placed on a standard rink in feet.
//!
//! InStat's drawings are not to scale, so each is calibrated from its own faceoff markings:
//! end-zone faceoff spots sit 22 ft either side of the middle and 20 ft out from the goal
//! line, which is 89 ft from centre ice; neutral-zone spots sit 20 ft from centre.

use std::ops::RangeInclusive;

use crate::layout;
use crate::model::{Feet, Jersey, RinkEvent, RinkEventKind, RinkPoint};
use crate::parse::common::LINE_TOLERANCE;
use crate::pdf::{Page, Rect, Rgb, Shape, Word};

const SPOT_ACROSS: f64 = 22.0;
const SPOT_OUT: f64 = 20.0;
const GOAL_LINE: f64 = 89.0;
const END_SPOT_ALONG: f64 = GOAL_LINE - SPOT_OUT;
const NEUTRAL_SPOT_ALONG: f64 = 20.0;
const HALF_LENGTH: f64 = 100.0;
const HALF_WIDTH: f64 = 42.5;
const BEHIND_GOAL_LINE: f64 = 11.0;
const GOAL_LINE_TO_BLUE_LINE: f64 = 64.0;

const FACEOFF_PINK: Rgb = Rgb { r: 0.98, g: 0.79, b: 0.79 };
const GOAL_RED: Rgb = Rgb { r: 1.0, g: 0.0, b: 0.0 };
/// End-zone faceoff circles on the half-rink charts are about 54 points across.
const CIRCLE_WIDTH: RangeInclusive<f64> = 50.0..=58.0;
/// Shot markers are outlined circles about 8 points across.
const SHOT_MARKER_WIDTH: RangeInclusive<f64> = 6.0..=10.0;

const CENTER_ICE_BLUE: Rgb = Rgb { r: 0.85, g: 0.94, b: 0.99 };
const WHITE: Rgb = Rgb { r: 1.0, g: 1.0, b: 1.0 };
/// On the player report's full-rink maps: the centre-ice dot, the faceoff spots and the
/// event markers (dots, rings and crosses about 3 points across).
const CENTER_DOT_WIDTH: RangeInclusive<f64> = 0.9..=1.6;
const SPOT_WIDTH: RangeInclusive<f64> = 1.2..=2.2;
const EVENT_MARKER_WIDTH: RangeInclusive<f64> = 2.0..=4.0;

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

/// Page-to-rink mapping for one full-rink map (our end on the left). InStat stretches the
/// drawing unevenly, so each axis is interpolated between the boards and the faceoff spots.
#[derive(Debug, Clone, PartialEq)]
struct FullRink {
    /// Page x → feet along the rink: end boards, spot columns, end boards.
    along: [(f64, f64); 6],
    /// Page y → feet across the rink: side boards, spot rows, side boards.
    across: [(f64, f64); 4],
    outline: Rect,
    center_x: f64,
}

/// Linear between the anchors around `value` (anchors sorted by page position), extended
/// past the outer ones.
fn piecewise(anchors: &[(f64, f64)], value: f64) -> f64 {
    let segments = anchors.windows(2);
    let last = segments.len().saturating_sub(1);
    let Some(pair) = segments.enumerate().find(|(i, w)| value <= w[1].0 || *i == last).map(|(_, w)| w) else {
        return 0.0;
    };
    let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
    y0 + (value - x0) * (y1 - y0) / (x1 - x0)
}

impl FullRink {
    fn point(&self, x: f64, y: f64) -> RinkPoint {
        RinkPoint {
            along: Feet(piecewise(&self.along, x)),
            across: Feet(piecewise(&self.across, y)),
        }
    }
}

/// Sorted values with near-equal ones (within `tolerance`) merged into the first.
fn distinct(mut values: Vec<f64>, tolerance: f64) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|b, a| (*b - *a).abs() <= tolerance);
    values
}

/// The rink's outline: the smallest dark shape around the centre-ice dot.
fn rink_outline(shapes: &[Shape], center: &Rect) -> Option<Rect> {
    shapes
        .iter()
        .filter(|s| is_dark(&s.fill) && s.bounds.width() > 100.0)
        .map(|s| s.bounds)
        .filter(|r| r.contains(center.center_x(), center.center_y()))
        .min_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
}

/// One full rink per centre-ice dot with an outline and four columns of two faceoff spots.
fn full_rinks(shapes: &[Shape]) -> Vec<FullRink> {
    let dots = |color: &Rgb, width: &RangeInclusive<f64>| -> Vec<Rect> {
        shapes
            .iter()
            .filter(|s| !s.stroked && s.fill.near(color) && width.contains(&s.bounds.width()))
            .map(|s| s.bounds)
            .collect()
    };
    let spots = dots(&FACEOFF_PINK, &SPOT_WIDTH);
    dots(&CENTER_ICE_BLUE, &CENTER_DOT_WIDTH)
        .iter()
        .filter_map(|center| {
            let outline = rink_outline(shapes, center)?;
            let inside: Vec<&Rect> = spots.iter().filter(|s| outline.contains(s.center_x(), s.center_y())).collect();
            let columns = distinct(inside.iter().map(|s| s.center_x()).collect(), 1.0);
            let rows = distinct(inside.iter().map(|s| s.center_y()).collect(), 1.0);
            let (&[a, b, c, d], &[top, bottom]) = (columns.as_slice(), rows.as_slice()) else {
                return None;
            };
            Some(FullRink {
                along: [
                    (outline.x0, -HALF_LENGTH),
                    (a, -END_SPOT_ALONG),
                    (b, -NEUTRAL_SPOT_ALONG),
                    (c, NEUTRAL_SPOT_ALONG),
                    (d, END_SPOT_ALONG),
                    (outline.x1, HALF_LENGTH),
                ],
                across: [(outline.top, -HALF_WIDTH), (top, -SPOT_ACROSS), (bottom, SPOT_ACROSS), (outline.bottom, HALF_WIDTH)],
                outline,
                center_x: center.center_x(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RinkMap {
    Recoveries,
    Losses,
    Hits,
    Battles,
}

const MAP_HEADINGS: [(RinkMap, &str); 4] = [
    (RinkMap::Recoveries, "Puck recoveries"),
    (RinkMap::Losses, "Puck losses"),
    (RinkMap::Hits, "Hits -"),
    (RinkMap::Battles, "Puck battles"),
];

/// The map whose heading sits closest above the rink, at its left end.
fn map_kind(headings: &[(RinkMap, f64, f64)], rink: &FullRink) -> Option<RinkMap> {
    let bounds = rink.outline;
    headings
        .iter()
        .filter(|(_, x, y)| *y < bounds.top && *x > bounds.x0 - 20.0 && *x < rink.center_x)
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(map, _, _)| *map)
}

fn is_dark(color: &Rgb) -> bool {
    color.r.max(color.g).max(color.b) < 0.2
}

/// Dots are the map's own events; hollow rings are hits taken; a lost battle is a cross
/// drawn as two outlined bars over the same spot.
fn map_events(map: RinkMap, rink: &FullRink, shapes: &[Shape]) -> Vec<RinkEvent> {
    let bounds = rink.outline;
    let mut events = Vec::new();
    let mut cross_bars: Vec<(f64, f64)> = Vec::new();
    let markers = shapes.iter().filter(|s| {
        EVENT_MARKER_WIDTH.contains(&s.bounds.width())
            && EVENT_MARKER_WIDTH.contains(&s.bounds.height())
            && bounds.contains(s.bounds.center_x(), s.bounds.center_y())
    });
    for marker in markers {
        let (x, y) = (marker.bounds.center_x(), marker.bounds.center_y());
        let dot = !marker.stroked && is_dark(&marker.fill);
        let kind = match map {
            RinkMap::Recoveries if dot => RinkEventKind::Recovery,
            RinkMap::Losses if dot => RinkEventKind::Loss,
            RinkMap::Hits if dot => RinkEventKind::Hit,
            RinkMap::Hits if marker.stroked && marker.fill.near(&WHITE) => RinkEventKind::HitTaken,
            RinkMap::Battles if dot => RinkEventKind::BattleWon,
            RinkMap::Battles if marker.stroked && is_dark(&marker.fill) => {
                cross_bars.push((x, y));
                continue;
            }
            _ => continue,
        };
        events.push(RinkEvent { kind, at: rink.point(x, y) });
    }
    cross_bars.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for pair in cross_bars.chunks(2) {
        let (x, y) = pair[0];
        events.push(RinkEvent {
            kind: RinkEventKind::BattleLost,
            at: rink.point(x, y),
        });
    }
    events
}

/// Every marker on a player page's recoveries, losses, hits and battles maps.
#[must_use]
pub fn rink_maps(page: &Page) -> Vec<RinkEvent> {
    let headings: Vec<(RinkMap, f64, f64)> = layout::lines(&page.words, LINE_TOLERANCE)
        .iter()
        .flat_map(|line| {
            MAP_HEADINGS
                .iter()
                .filter_map(|(map, phrase)| line.find_phrase(phrase).map(|x| (*map, x, line.y)))
        })
        .collect();
    full_rinks(&page.shapes)
        .iter()
        .filter_map(|rink| Some(map_events(map_kind(&headings, rink)?, rink, &page.shapes)))
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{rink_maps, shooting_chart};
    use crate::model::{Jersey, RinkEventKind};
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

    const BLACK: Rgb = Rgb { r: 0.0, g: 0.0, b: 0.0 };
    const DARK: Rgb = Rgb { r: 0.14, g: 0.12, b: 0.13 };
    const ICE_BLUE: Rgb = Rgb { r: 0.85, g: 0.94, b: 0.99 };

    /// A full rink 200 points long and 85 tall at (0, 100), drawn to scale (1 pt per foot),
    /// with the "Puck battles" heading above it.
    fn battle_map(markers: Vec<Shape>) -> Page {
        let outline = Shape {
            bounds: Rect { x0: 0.0, x1: 200.0, top: 100.0, bottom: 185.0 },
            fill: DARK,
            stroked: false,
        };
        let mut shapes = vec![outline, shape(99.4, 141.9, 1.2, ICE_BLUE, false)];
        for x in [31.0, 80.0, 120.0, 169.0] {
            for y in [120.5, 164.5] {
                shapes.push(shape(x - 0.75, y - 0.75, 1.5, PINK, false));
            }
        }
        shapes.extend(markers);
        Page {
            number: 3,
            words: vec![word("Puck", 0.0, 18.0, 60.0), word("battles", 20.0, 46.0, 60.0)],
            fills: Vec::new(),
            clips: Vec::new(),
            shapes,
        }
    }

    #[test]
    fn full_rink_markers_are_placed_from_the_boards_and_faceoff_spots() {
        let won = shape(167.5, 163.0, 3.0, BLACK, false);

        let events = rink_maps(&battle_map(vec![won]));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RinkEventKind::BattleWon);
        assert!((events[0].at.along.0 - 69.0).abs() < 1e-9);
        assert!((events[0].at.across.0 - 22.0).abs() < 1e-9);
    }

    #[test]
    fn a_cross_of_two_outlined_bars_is_one_lost_battle() {
        let bar = shape(50.0, 130.0, 3.0, BLACK, true);

        let events = rink_maps(&battle_map(vec![bar, bar]));

        let kinds: Vec<RinkEventKind> = events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [RinkEventKind::BattleLost]);
    }

    #[test]
    fn legend_markers_above_the_rink_are_ignored() {
        let legend = shape(60.0, 90.0, 3.0, BLACK, false);

        assert!(rink_maps(&battle_map(vec![legend])).is_empty());
    }
}
