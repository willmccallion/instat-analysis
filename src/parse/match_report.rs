//! The InStat "Match report", read only for what the event export lacks.
//!
//! That is both teams' xG and possession, our players' hits against, shot types and kinds of
//! attack, and who InStat lists as forwards and defencemen.

use crate::error::Error;
use crate::layout;
use crate::model::{Date, TeamName, TeamPrefix};
use crate::parse::common::{LINE_TOLERANCE, PAGE_RIGHT, PlayerRow, page_heading, player_table, require_phrase};
use crate::parse::lines::{self, ListedPlayer};
use crate::parse::team_stats::{self, TeamStatsPage};
use crate::pdf::Page;

const SECTION: &str = "match report";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Title {
    pub teams: [TeamName; 2],
    pub score: (u32, u32),
    pub date: Date,
}

/// Both teams' stats page and our players' tables.
#[derive(Debug, Clone)]
pub struct MatchReport {
    pub title: Title,
    /// Index of our team in `title.teams`.
    pub our_index: usize,
    pub team_stats: TeamStatsPage,
    /// Our players' main statistics table.
    pub players: Vec<PlayerRow>,
    /// Our players' shots table.
    pub shots: Vec<PlayerRow>,
    /// Our players in the forward lines and defence pairs; empty without a lines page.
    pub lines: Vec<ListedPlayer>,
}

/// Index of our team in the title (0 = listed first).
pub fn our_index(title: &Title, team: &TeamPrefix) -> Result<usize, Error> {
    match (team.matches(&title.teams[0]), team.matches(&title.teams[1])) {
        (true, false) => Ok(0),
        (false, true) => Ok(1),
        (both, _) => Err(Error::WrongTeam {
            team: team.as_str().to_owned(),
            matchup: format!("{} vs {}", title.teams[0].0, title.teams[1].0),
            both,
        }),
    }
}

/// Whether the first page is a match report cover.
#[must_use]
pub fn is_match_report(pages: &[Page]) -> bool {
    pages
        .first()
        .is_some_and(|p| p.joined_text().starts_with("MATCH REPORT"))
}

/// Parses the cover line `"SSAC LIONS U15 AAA 3:2 NORTH STARS U15 AAA"` and the date.
pub fn parse_cover(page: &Page) -> Result<Title, Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let (index, score_word, score) = lines
        .iter()
        .enumerate()
        .find_map(|(i, line)| {
            line.words.iter().enumerate().find_map(|(j, w)| {
                let (a, b) = w.text.split_once(':')?;
                Some((i, j, (a.parse().ok()?, b.parse().ok()?)))
            })
        })
        .ok_or_else(|| Error::parse(SECTION, "no score on cover"))?;
    let words: Vec<&str> = lines[index].words.iter().map(|w| w.text.as_str()).collect();
    let first = TeamName::new(&words[..score_word].join(" "));
    let second = TeamName::new(&words[score_word + 1..].join(" "));
    let date = lines[index + 1..]
        .iter()
        .find_map(|l| l.words.first().and_then(|w| Date::parse_dotted(&w.text)))
        .ok_or_else(|| Error::parse(SECTION, "no date on cover"))?;
    Ok(Title {
        teams: [first, second],
        score,
        date,
    })
}

pub fn parse(pages: &[Page], team: &TeamPrefix) -> Result<MatchReport, Error> {
    let cover = pages.first().ok_or_else(|| Error::parse(SECTION, "empty document"))?;
    let title = parse_cover(cover)?;
    let our_index = our_index(&title, team)?;
    let our_name = &title.teams[our_index];
    let (mut team_stats, mut players, mut shots) = (None, None, None);
    let mut listed = Vec::new();
    for page in &pages[1..] {
        let Some(heading) = page_heading(page) else {
            continue;
        };
        if heading.title == "TEAMS STATS" {
            team_stats = Some(team_stats::parse(page)?);
        } else if heading.team.as_ref() == Some(our_name) {
            match heading.title.as_str() {
                "PLAYERS' STATS" => players = Some(table_under(page, "Main statistics", "players' stats")?),
                "SHOTS" => shots = Some(table_under(page, "Shots stats", "shots")?),
                "LINES STATS" => listed = lines::parse(page)?,
                _ => {}
            }
        }
    }
    let missing = |what: &str| Error::parse(SECTION, format!("no {what} page"));
    Ok(MatchReport {
        title,
        our_index,
        team_stats: team_stats.ok_or_else(|| missing("TEAMS STATS"))?,
        players: players.ok_or_else(|| missing("PLAYERS' STATS"))?,
        shots: shots.ok_or_else(|| missing("SHOTS"))?,
        lines: listed,
    })
}

/// The per-player table under the heading `phrase`.
fn table_under(page: &Page, phrase: &str, section: &'static str) -> Result<Vec<PlayerRow>, Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let (y, x) = require_phrase(&lines, phrase, section)?;
    player_table(&page.words, x, PAGE_RIGHT, y, section)
}
