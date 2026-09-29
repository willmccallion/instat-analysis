//! The InStat "Match report": one team-stats page plus eight pages per team (ours parsed).

use crate::error::Error;
use crate::layout;
use crate::model::{Date, TeamName, TeamPrefix};
use crate::parse::common::{
    LINE_TOLERANCE, PAGE_RIGHT, PlayerRow, page_heading, player_table, require_phrase,
};
use crate::parse::lines::{self, RawUnit};
use crate::parse::matrix::{self, RawMatrix};
use crate::parse::team_stats::{self, TeamStatsPage};
use crate::parse::timeline::{self, Timeline};
use crate::pdf::Page;

const SECTION: &str = "match report";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Title {
    pub teams: [TeamName; 2],
    pub score: (u32, u32),
    pub date: Date,
}

/// Per-player tables for one team; each list is in the report's row order.
#[derive(Debug, Clone, Default)]
pub struct PlayerTables {
    pub main: Vec<PlayerRow>,
    pub challenges: Vec<PlayerRow>,
    pub turnovers: Vec<PlayerRow>,
    pub entries: Vec<PlayerRow>,
    pub shots: Vec<PlayerRow>,
    pub challenges_by_zone: Vec<PlayerRow>,
}

#[derive(Debug, Clone, Default)]
pub struct TeamPages {
    pub tables: PlayerTables,
    pub units: Vec<RawUnit>,
    pub timeline: Option<Timeline>,
    pub passes: Option<RawMatrix>,
    /// Puck battles won—lost against each opponent skater.
    pub battles: Option<RawMatrix>,
    /// Hits given—taken against each opponent skater.
    pub hits: Option<RawMatrix>,
}

/// Our team's pages in full; of the opponent's, only team-level stats and their shots table.
#[derive(Debug, Clone)]
pub struct MatchReport {
    pub title: Title,
    /// Index of our team in `title.teams`.
    pub our_index: usize,
    pub team_stats: TeamStatsPage,
    pub ours: TeamPages,
    pub opponent_shots: Vec<PlayerRow>,
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
    let cover = pages
        .first()
        .ok_or_else(|| Error::parse(SECTION, "empty document"))?;
    let title = parse_cover(cover)?;
    let our_index = our_index(&title, team)?;
    let our_name = &title.teams[our_index];
    let mut team_stats = None;
    let mut ours = TeamPages::default();
    let mut opponent_shots = Vec::new();
    for page in &pages[1..] {
        let Some(heading) = page_heading(page) else {
            continue;
        };
        if heading.title == "TEAMS STATS" {
            team_stats = Some(team_stats::parse(page)?);
            continue;
        }
        if heading.team.as_ref() == Some(our_name) {
            parse_team_page(page, &heading.title, &mut ours)?;
        } else if heading.title == "SHOTS" {
            opponent_shots = shots_table(page)?;
        }
    }
    let team_stats = team_stats.ok_or_else(|| Error::parse(SECTION, "no TEAMS STATS page"))?;
    Ok(MatchReport {
        title,
        our_index,
        team_stats,
        ours,
        opponent_shots,
    })
}

fn parse_team_page(page: &Page, title: &str, team: &mut TeamPages) -> Result<(), Error> {
    match title {
        "PLAYERS' STATS" => parse_players_stats(page, &mut team.tables)?,
        "LINES STATS" => team.units = lines::parse(page)?,
        "GAME TIME DISTRIBUTION" => team.timeline = Some(timeline::parse(page)?),
        "SHOTS" => team.tables.shots = shots_table(page)?,
        "CHALLENGES" => {
            let lines = layout::lines(&page.words, LINE_TOLERANCE);
            let (y, x) = require_phrase(&lines, "Challenges", "challenges")?;
            team.tables.challenges_by_zone =
                player_table(&page.words, x, PAGE_RIGHT, y, "challenges")?;
        }
        "PASSES DISTRIBUTION" => team.passes = Some(matrix::parse(page)?),
        "CHALLENGE DISTRIBUTION" => team.battles = Some(matrix::parse(page)?),
        "HITS DISTRIBUTION" => team.hits = Some(matrix::parse(page)?),
        _ => {}
    }
    Ok(())
}

fn shots_table(page: &Page) -> Result<Vec<PlayerRow>, Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let (y, x) = require_phrase(&lines, "Shots stats", "shots")?;
    player_table(&page.words, x, PAGE_RIGHT, y, "shots")
}

fn parse_players_stats(page: &Page, tables: &mut PlayerTables) -> Result<(), Error> {
    let lines = layout::lines(&page.words, LINE_TOLERANCE);
    let (main_y, main_x) = require_phrase(&lines, "Main statistics", "players' stats")?;
    tables.main = player_table(&page.words, main_x, PAGE_RIGHT, main_y, "players' stats")?;
    let (sub_y, challenges_x) = require_phrase(&lines, "Challenges", "players' stats")?;
    let (_, turnovers_x) = require_phrase(&lines, "Turnovers and takeaways", "players' stats")?;
    let (_, entries_x) = require_phrase(&lines, "Entries", "players' stats")?;
    let label_offset = 1.0;
    tables.challenges = player_table(
        &page.words,
        challenges_x,
        turnovers_x - label_offset,
        sub_y,
        "challenges table",
    )?;
    tables.turnovers = player_table(
        &page.words,
        turnovers_x - label_offset,
        entries_x - label_offset,
        sub_y,
        "turnovers table",
    )?;
    tables.entries = player_table(
        &page.words,
        entries_x - label_offset,
        PAGE_RIGHT,
        sub_y,
        "entries table",
    )?;
    Ok(())
}
