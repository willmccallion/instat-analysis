//! The InStat "Player report": one page per player with game stats and recent-game history.

use crate::cell::Cell;
use crate::error::Error;
use crate::layout::{self, Line};
use crate::model::{Date, Jersey, NetArea, ShotZone, TeamPrefix};
use crate::parse::common::LINE_TOLERANCE;
use crate::parse::diagrams;
use crate::parse::match_report::{Title, our_index, parse_cover};
use crate::pdf::{Page, Rect, Word};

const SECTION: &str = "player report";

/// Statistics block columns on a player page.
const STAT_LABEL_X: f64 = 30.0;
const STAT_GAME_X: f64 = 115.0;
/// Fallback when the "Season average" header is missing; it moves a few points per page.
const STAT_SEASON_X: f64 = 155.0;
const STAT_BLOCK_END_X: f64 = 225.0;
/// History table starts at the date column.
const HISTORY_DATE_X: f64 = 628.0;
const HISTORY_OPPONENT_X: f64 = 645.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Skater,
    Goalie,
}

#[derive(Debug, Clone)]
pub struct RawHistoryRow {
    pub day: u8,
    pub month: u8,
    pub opponent: String,
    pub cells: Vec<(HistoryColumn, Cell)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryColumn {
    InstatIndex,
    TimeOnIce,
    Goals,
    Assists,
    Shots,
    PlusMinus,
    ShotsAgainst,
    Saves,
    GoalsAgainst,
    SavePercent,
}

/// One of our players' pages.
#[derive(Debug, Clone)]
pub struct PlayerPage {
    pub kind: PageKind,
    pub full_name: String,
    pub jersey: Option<Jersey>,
    /// (label, game value, season value) from the Statistics block.
    pub stats: Vec<(String, String, String)>,
    pub history: Vec<RawHistoryRow>,
    /// Each part of the net: shots / saves on goalie pages, shots on goal / goals on
    /// skater pages; this game only.
    pub net: Vec<(NetArea, (u32, u32))>,
    /// Goalie pages: shots / saves from each of InStat's seven zones, this game.
    pub zones: Vec<(ShotZone, (u32, u32))>,
}

impl PlayerPage {
    #[must_use]
    pub fn stat(&self, label: &str) -> Option<Cell> {
        self.stats
            .iter()
            .find(|(l, _, _)| l == label)
            .map(|(_, game, _)| Cell::parse(game))
    }
}

#[derive(Debug, Clone)]
pub struct PlayersReport {
    pub title: Title,
    pub players: Vec<PlayerPage>,
}

#[must_use]
pub fn is_players_report(pages: &[Page]) -> bool {
    pages
        .first()
        .is_some_and(|p| p.joined_text().starts_with("PLAYER REPORT"))
}

pub fn parse(pages: &[Page], team: &TeamPrefix) -> Result<PlayersReport, Error> {
    let cover = pages
        .first()
        .ok_or_else(|| Error::parse(SECTION, "empty document"))?;
    let title = parse_cover(cover)?;
    let ours = our_index(&title, team)?;
    let team_of_page = contents_team_map(cover, &title)?;
    let mut players = Vec::new();
    for page in &pages[1..] {
        let is_ours = team_of_page
            .iter()
            .any(|&(n, team)| n == page.number && team == ours);
        if is_ours {
            let player = parse_player_page(page)
                .map_err(|e| Error::parse(SECTION, format!("page {}: {e}", page.number)))?;
            players.push(player);
        }
    }
    Ok(PlayersReport { title, players })
}

/// Maps page numbers to team index using the cover's two-column table of contents.
fn contents_team_map(cover: &Page, title: &Title) -> Result<Vec<(u32, usize)>, Error> {
    let lines = layout::lines(&cover.words, LINE_TOLERANCE);
    let second_team_x = lines
        .iter()
        .find_map(|l| {
            let text = l.text();
            let first = &title.teams[0].0;
            let second = &title.teams[1].0;
            (text.starts_with(first.as_str())
                && text.contains(second.as_str())
                && !text.contains(':'))
            .then(|| l.find_phrase(second.split_whitespace().next().unwrap_or_default()))
            .flatten()
            .filter(|x| *x > 100.0)
        })
        .ok_or_else(|| Error::parse(SECTION, "table of contents header not found"))?;
    let header_y = lines
        .iter()
        .find(|l| {
            let text = l.text();
            text.starts_with(title.teams[0].0.as_str()) && !text.contains(':') && l.first_x() < 40.0
        })
        .map_or(0.0, |l| l.y);
    let map: Vec<(u32, usize)> = lines
        .iter()
        .filter(|l| l.y > header_y + 1.0)
        .flat_map(|l| l.words.iter())
        .filter_map(|w| {
            let page_number = w.text.parse::<u32>().ok()?;
            (page_number > 1).then_some((page_number, usize::from(w.x0 > second_team_x)))
        })
        .collect();
    if map.is_empty() {
        return Err(Error::parse(SECTION, "empty table of contents"));
    }
    Ok(map)
}

fn parse_player_page(page: &Page) -> Result<PlayerPage, Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let heading = lines
        .iter()
        .filter(|l| l.y < 30.0)
        .nth(1)
        .ok_or_else(|| Error::parse(SECTION, "no page heading"))?;
    let upper_name = heading
        .words
        .iter()
        .filter(|w| w.text.parse::<u32>().is_err())
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let (jersey, full_name) =
        identity_line(&lines).unwrap_or_else(|| (None, title_case(&upper_name)));
    let stats = stat_block(&lines);
    let kind = if stats
        .iter()
        .any(|(label, _, _)| label.starts_with("Shots against"))
    {
        PageKind::Goalie
    } else {
        PageKind::Skater
    };
    let history = history(&page.words, kind)?;
    let (net, zones) = match kind {
        PageKind::Goalie => goalie_diagrams(&page.words).map_or_else(Default::default, |regions| {
            (
                diagrams::net_grid(&page.words, &regions.net).unwrap_or_default(),
                diagrams::zone_grid(&page.words, &regions.rink).unwrap_or_default(),
            )
        }),
        PageKind::Skater => (
            skater_net_region(&lines)
                .and_then(|region| diagrams::net_grid(&page.words, &region))
                .unwrap_or_default(),
            Vec::new(),
        ),
    };
    Ok(PlayerPage {
        kind,
        full_name,
        jersey,
        stats,
        history,
        net,
        zones,
    })
}

/// The "Shots on goal" net diagram: below its heading, left of "Shift log", above the
/// "Puck losses" map heading.
fn skater_net_region(lines: &[Line<'_>]) -> Option<Rect> {
    let (heading_y, x0) = lines.iter().find_map(|l| l.find_phrase("Shots on goal").map(|x| (l.y, x)))?;
    let x1 = lines.iter().find_map(|l| l.find_phrase("Shift log"))?;
    let bottom = lines
        .iter()
        .filter(|l| l.y > heading_y)
        .find_map(|l| l.find_phrase("Puck losses").filter(|x| (x - x0).abs() < 5.0).map(|_| l.y))?;
    Some(Rect {
        x0: x0 - 10.0,
        x1: x1 - 5.0,
        top: heading_y + 5.0,
        bottom: bottom - 5.0,
    })
}

/// Where this game's net and rink diagrams sit on a goalie page.
struct GoalieDiagrams {
    net: Rect,
    rink: Rect,
}

/// The goalie page stacks three diagrams (body, net, rink) under "GAME", with the season's
/// copies beside them under "SEASON".
fn goalie_diagrams(words: &[Word]) -> Option<GoalieDiagrams> {
    let mut game_labels: Vec<&Word> = words.iter().filter(|w| w.text == "GAME").collect();
    game_labels.sort_by(|a, b| a.top.total_cmp(&b.top));
    let [_, net_label, rink_label] = game_labels.as_slice() else {
        return None;
    };
    let season_x = words.iter().find(|w| w.text == "SEASON")?.center_x();
    let half_width = (season_x - net_label.center_x()).abs() / 2.0;
    let column = |top: f64, bottom: f64| Rect {
        x0: net_label.center_x() - half_width,
        x1: net_label.center_x() + half_width,
        top,
        bottom,
    };
    Some(GoalieDiagrams {
        net: column(net_label.bottom, rink_label.top),
        rink: column(rink_label.bottom, f64::INFINITY),
    })
}

/// Skater pages start with `"16 Sam Carter"` at the top left.
fn identity_line(lines: &[Line<'_>]) -> Option<(Option<Jersey>, String)> {
    lines
        .iter()
        .filter(|l| l.y > 40.0 && l.y < 75.0)
        .find_map(|line| {
            let first = line.words.first()?;
            let number: u16 = first.text.parse().ok()?;
            let name: Vec<&str> = line
                .words
                .iter()
                .skip(1)
                .take_while(|w| w.x0 < STAT_BLOCK_END_X)
                .map(|w| w.text.as_str())
                .collect();
            (!name.is_empty()).then(|| (Some(Jersey(number)), name.join(" ")))
        })
}

fn title_case(upper: &str) -> String {
    upper
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn stat_block(lines: &[Line<'_>]) -> Vec<(String, String, String)> {
    let season_x = lines
        .iter()
        .flat_map(|l| l.words.iter())
        .find(|w| w.text == "Season" && w.x0 < STAT_BLOCK_END_X)
        .map_or(STAT_SEASON_X, |w| w.x0 - 3.0);
    lines
        .iter()
        .filter(|l| l.y > 80.0)
        .filter_map(|line| {
            let label = line.text_between(STAT_LABEL_X, STAT_GAME_X);
            let game = line.text_between(STAT_GAME_X, season_x);
            let season = line.text_between(season_x, STAT_BLOCK_END_X);
            let is_row = label.starts_with(|c: char| c.is_alphabetic()) && !game.is_empty();
            is_row.then_some((label, game, season))
        })
        .collect()
}

fn day_month(text: &str) -> Option<(u8, u8)> {
    let (day, month) = text.split_once('.')?;
    let day: u8 = day.parse().ok()?;
    let month: u8 = month.parse().ok()?;
    ((1..=31).contains(&day) && (1..=12).contains(&month)).then_some((day, month))
}

/// x of the `nth` header word starting with `prefix` (headers sometimes wrap mid-word).
fn anchor_x(words: &[&Word], prefix: &str, nth: usize) -> Option<f64> {
    let mut found: Vec<&&Word> = words
        .iter()
        .filter(|w| w.text.starts_with(prefix))
        .collect();
    found.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    found.get(nth).map(|w| w.x0)
}

fn history_columns(header: &[&Word], kind: PageKind) -> Result<Vec<(HistoryColumn, f64)>, Error> {
    use HistoryColumn as H;
    let wanted: Vec<(H, &str, usize)> = match kind {
        PageKind::Skater => vec![
            (H::InstatIndex, "InStat", 0),
            (H::TimeOnIce, "Time", 0),
            (H::Goals, "Goal", 0),
            (H::Assists, "Assist", 0),
            (H::Shots, "Shot", 0),
            (H::PlusMinus, "+", 0),
        ],
        PageKind::Goalie => vec![
            (H::InstatIndex, "InStat", 0),
            (H::TimeOnIce, "Time", 0),
            (H::ShotsAgainst, "Shot", 0),
            (H::Saves, "Save", 0),
            (H::GoalsAgainst, "Goal", 0),
            (H::SavePercent, "Save", 1),
        ],
    };
    let mut columns: Vec<(H, f64)> = wanted
        .into_iter()
        .map(|(column, text, nth)| {
            anchor_x(header, text, nth)
                .map(|x| (column, x))
                .ok_or_else(|| Error::parse(SECTION, format!("history column {text:?} missing")))
        })
        .collect::<Result<_, _>>()?;
    columns.sort_by(|a, b| a.1.total_cmp(&b.1));
    Ok(columns)
}

fn history(words: &[Word], kind: PageKind) -> Result<Vec<RawHistoryRow>, Error> {
    let right: Vec<&Word> = words.iter().filter(|w| w.x0 >= HISTORY_DATE_X).collect();
    let Some(title_y) = right
        .iter()
        .find(|w| w.text == "Comparison")
        .map(|w| w.center_y())
    else {
        return Ok(Vec::new());
    };
    let mut date_words: Vec<&&Word> = right
        .iter()
        .filter(|w| {
            w.center_y() > title_y && w.x0 < HISTORY_OPPONENT_X && day_month(&w.text).is_some()
        })
        .collect();
    date_words.sort_by(|a, b| a.top.total_cmp(&b.top));
    let Some(first_date) = date_words.first() else {
        return Ok(Vec::new());
    };
    let header: Vec<&Word> = right
        .iter()
        .copied()
        .filter(|w| w.center_y() > title_y + 2.0 && w.center_y() < first_date.center_y() - 2.0)
        .collect();
    let columns = history_columns(&header, kind)?;
    let first_column_x = columns.first().map_or(f64::INFINITY, |c| c.1);
    let slack = 2.0;
    let mut rows = Vec::new();
    for (i, date) in date_words.iter().enumerate() {
        let Some((day, month)) = day_month(&date.text) else {
            continue;
        };
        let y = date.center_y();
        let previous_y = i
            .checked_sub(1)
            .map_or(f64::NEG_INFINITY, |p| date_words[p].center_y());
        let next_y = date_words
            .get(i + 1)
            .map_or(f64::INFINITY, |n| n.center_y());
        let row_words: Vec<&Word> = right
            .iter()
            .copied()
            .filter(|w| (w.center_y() - y).abs() < 1.5)
            .collect();
        let mut opponent_words: Vec<&Word> = right
            .iter()
            .copied()
            .filter(|w| w.x0 >= HISTORY_OPPONENT_X && w.x0 < first_column_x - slack)
            .filter(|w| {
                let cy = w.center_y();
                let nearest_is_this = (cy - y).abs() <= (cy - previous_y).abs()
                    && (cy - y).abs() < (cy - next_y).abs();
                (cy - y).abs() < 7.0 && nearest_is_this
            })
            .collect();
        opponent_words.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
        let opponent = opponent_words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let cells = columns
            .iter()
            .enumerate()
            .map(|(c, (column, x))| {
                let end = columns
                    .get(c + 1)
                    .map_or(f64::INFINITY, |next| next.1 - slack);
                let text = row_words
                    .iter()
                    .filter(|w| w.x0 >= x - slack && w.x0 < end)
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                (*column, Cell::parse(&text))
            })
            .collect();
        rows.push(RawHistoryRow {
            day,
            month,
            opponent,
            cells,
        });
    }
    Ok(rows)
}

/// Resolves a printed `dd.mm` to the latest date not after `reference`.
#[must_use]
pub fn infer_year(day: u8, month: u8, reference: Date) -> Date {
    let same_year = Date {
        year: reference.year,
        month,
        day,
    };
    if same_year.ordinal() <= reference.ordinal() + 1 {
        same_year
    } else {
        Date {
            year: reference.year - 1,
            month,
            day,
        }
    }
}
