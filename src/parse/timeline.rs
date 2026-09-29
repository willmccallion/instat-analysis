//! The GAME TIME DISTRIBUTION page: every shift as a bar on a per-period timeline, plus
//! goal markers and man-advantage bands drawn over it.

use crate::error::Error;
use crate::model::{Interval, PERIOD_SECONDS, Seconds, Strength};
use crate::parse::common::RowLabel;
use crate::pdf::{Page, Rect, Word};

const SECTION: &str = "game time distribution";

/// Height of a player's shift-bar row and of each period panel, in points.
const ROW_HEIGHT: f64 = 7.2;
const ROW_HEIGHT_TOLERANCE: f64 = 0.3;
/// Goal marker boxes are ~9.6 x 6.5 points; goal lines ~0.7 points wide.
const MARKER_MIN_WIDTH: f64 = 8.0;
const MARKER_MAX_WIDTH: f64 = 12.0;
const MARKER_MAX_HEIGHT: f64 = 7.0;
const BAND_MIN_HEIGHT: f64 = 100.0;
const BAND_MIN_WIDTH: f64 = 1.5;
/// Row labels sit left of the timeline.
const LABEL_MAX_X: f64 = 80.0;

#[derive(Debug, Clone)]
pub struct TimelineRow {
    pub label: RowLabel,
    pub group: Option<String>,
    pub shifts: Vec<Interval>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoalMarker {
    /// Where InStat drew the marker; the goal itself is at or just before this.
    pub marker_time: Seconds,
    pub score_after: (u32, u32),
}

/// A special-teams band, from this page's team's point of view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub interval: Interval,
    pub strength: Strength,
}

#[derive(Debug, Clone)]
pub struct Timeline {
    pub rows: Vec<TimelineRow>,
    pub goals: Vec<GoalMarker>,
    pub bands: Vec<Band>,
    pub length: Seconds,
}

/// Maps page x to game seconds, one panel per period.
#[derive(Debug, Clone)]
struct Panels {
    starts: Vec<f64>,
    width: f64,
}

impl Panels {
    fn from_clips(clips: &[Rect]) -> Result<Self, Error> {
        let mut panel_rects: Vec<&Rect> = clips
            .iter()
            .filter(|r| (r.height() - ROW_HEIGHT).abs() < ROW_HEIGHT_TOLERANCE && r.width() > 100.0)
            .collect();
        panel_rects.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut starts: Vec<f64> = Vec::new();
        for rect in &panel_rects {
            if starts.last().is_none_or(|last| (rect.x0 - last).abs() > 1.0) {
                starts.push(rect.x0);
            }
        }
        let width = panel_rects
            .iter()
            .map(|r| r.width())
            .fold(0.0_f64, f64::max);
        if starts.len() < 3 || width <= 0.0 {
            return Err(Error::parse(SECTION, "period panels not found"));
        }
        Ok(Self { starts, width })
    }

    fn time_at(&self, x: f64) -> Option<Seconds> {
        let tolerance = 1.0;
        self.starts.iter().enumerate().find_map(|(period, start)| {
            let inside = x >= start - tolerance && x <= start + self.width + tolerance;
            inside.then(|| {
                let fraction = ((x - start) / self.width).clamp(0.0, 1.0);
                Seconds((period as f64).mul_add(PERIOD_SECONDS, fraction * PERIOD_SECONDS))
            })
        })
    }

    fn length(&self) -> Seconds {
        Seconds(self.starts.len() as f64 * PERIOD_SECONDS)
    }
}

pub fn parse(page: &Page) -> Result<Timeline, Error> {
    let panels = Panels::from_clips(&page.clips)?;
    Ok(Timeline {
        rows: rows(page, &panels)?,
        goals: goal_markers(page, &panels)?,
        bands: bands(page, &panels),
        length: panels.length(),
    })
}

fn rows(page: &Page, panels: &Panels) -> Result<Vec<TimelineRow>, Error> {
    let mut bars: Vec<&Rect> = page
        .fills
        .iter()
        .filter(|r| (r.height() - ROW_HEIGHT).abs() < ROW_HEIGHT_TOLERANCE && r.x0 > LABEL_MAX_X)
        .collect();
    bars.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
    let mut grouped: Vec<(f64, Vec<&Rect>)> = Vec::new();
    for bar in bars {
        match grouped.last_mut() {
            Some((top, members)) if (bar.top - *top).abs() < 1.0 => members.push(bar),
            _ => grouped.push((bar.top, vec![bar])),
        }
    }
    let headings = group_headings(&page.words);
    grouped
        .into_iter()
        .map(|(top, bars)| {
            let center = top + ROW_HEIGHT / 2.0;
            let label = row_label(&page.words, center)?;
            let group = headings
                .iter()
                .rfind(|(y, _)| *y < center)
                .map(|(_, name)| name.clone());
            let shifts = bars
                .iter()
                .filter_map(|bar| {
                    Some(Interval {
                        start: panels.time_at(bar.x0)?,
                        end: panels.time_at(bar.x1)?,
                    })
                })
                .collect();
            Ok(TimelineRow {
                label,
                group,
                shifts,
            })
        })
        .collect()
}

/// The number is printed level with the bar (InStat sometimes omits it), the surname below.
fn row_label(words: &[Word], bar_center: f64) -> Result<RowLabel, Error> {
    let left: Vec<&Word> = words.iter().filter(|w| w.x0 < LABEL_MAX_X).collect();
    let number = left
        .iter()
        .filter(|w| w.text.parse::<u16>().is_ok())
        .filter(|w| (w.center_y() - bar_center).abs() < 4.0)
        .min_by(|a, b| {
            (a.center_y() - bar_center)
                .abs()
                .total_cmp(&(b.center_y() - bar_center).abs())
        });
    let mut surname_words: Vec<&&Word> = left
        .iter()
        .filter(|w| {
            let dy = w.center_y() - bar_center;
            dy > 2.0 && dy < 9.0 && w.text.starts_with(|c: char| c.is_alphabetic())
        })
        .collect();
    surname_words.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
    let surname = surname_words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    RowLabel::from_parts(number.map(|w| w.text.as_str()), &surname)
        .ok_or_else(|| Error::parse(SECTION, format!("no surname near y={bar_center:.1}")))
}

/// Upper-case headings such as "FIRST LINE" or "OTHER PLAYERS" in the label column.
fn group_headings(words: &[Word]) -> Vec<(f64, String)> {
    let mut result: Vec<(f64, String)> = Vec::new();
    let mut candidates: Vec<&Word> = words
        .iter()
        .filter(|w| w.x0 < LABEL_MAX_X && w.top > 60.0)
        .filter(|w| w.text.len() > 1 && w.text.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    candidates.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
    for word in candidates {
        match result.last_mut() {
            Some((y, text)) if (word.center_y() - *y).abs() < 1.5 => {
                text.push(' ');
                text.push_str(&word.text);
            }
            _ => result.push((word.center_y(), word.text.clone())),
        }
    }
    result
}

fn parse_score(text: &str) -> Option<(u32, u32)> {
    let (a, b) = text.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn goal_markers(page: &Page, panels: &Panels) -> Result<Vec<GoalMarker>, Error> {
    let mut boxes: Vec<&Rect> = page
        .clips
        .iter()
        .filter(|r| {
            (MARKER_MIN_WIDTH..MARKER_MAX_WIDTH).contains(&r.width()) && r.height() < MARKER_MAX_HEIGHT
        })
        .collect();
    boxes.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    boxes
        .into_iter()
        .map(|marker| {
            let score = page
                .words
                .iter()
                .filter(|w| w.center_y() > marker.top - 2.0 && w.center_y() < marker.bottom + 2.0)
                .filter(|w| w.x0 >= marker.x0 - 1.0 && w.x0 <= marker.x1)
                .find_map(|w| parse_score(&w.text))
                .ok_or_else(|| Error::parse(SECTION, "goal marker without score"))?;
            let marker_time = panels
                .time_at(marker.center_x())
                .ok_or_else(|| Error::parse(SECTION, "goal marker outside timeline"))?;
            Ok(GoalMarker {
                marker_time,
                score_after: score,
            })
        })
        .collect()
}

fn bands(page: &Page, panels: &Panels) -> Vec<Band> {
    page.clips
        .iter()
        .filter(|r| r.height() > BAND_MIN_HEIGHT && r.width() > BAND_MIN_WIDTH && r.width() < 100.0)
        .filter_map(|band| {
            let label = page
                .words
                .iter()
                .filter(|w| (w.text == "PP" || w.text == "SH") && w.top < band.top + 10.0)
                .min_by(|a, b| {
                    (a.center_x() - band.center_x())
                        .abs()
                        .total_cmp(&(b.center_x() - band.center_x()).abs())
                })?;
            let strength = if label.text == "PP" {
                Strength::PowerPlay
            } else {
                Strength::ShortHanded
            };
            Some(Band {
                interval: Interval {
                    start: panels.time_at(band.x0)?,
                    end: panels.time_at(band.x1)?,
                },
                strength,
            })
        })
        .collect()
}
