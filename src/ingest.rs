//! Turning dropped files into documents: InStat's event exports (CSV) and its optional match
//! and player reports (PDF).

use crate::error::Error;
use crate::model::TeamPrefix;
use crate::parse::events::{self, EventFile};
use crate::parse::match_report::{self, MatchReport, Title};
use crate::parse::players_report::{self, PlayersReport};
use crate::pdf;

#[derive(Debug, Clone)]
pub enum Document {
    Match(Box<MatchReport>),
    Players(Box<PlayersReport>),
    /// One of the two event-export files (players or team), for any game.
    Events(Box<EventFile>),
}

impl Document {
    #[must_use]
    pub fn title(&self) -> &Title {
        match self {
            Self::Match(m) => &m.title,
            Self::Players(p) => &p.title,
            Self::Events(e) => &e.title,
        }
    }
}

/// Reads an uploaded file: an event-export CSV (whose date and score come from `file_name`)
/// or a PDF report.
pub fn parse_upload(bytes: &[u8], file_name: &str, team: &TeamPrefix) -> Result<Document, Error> {
    if events::is_event_export(bytes) {
        return Ok(Document::Events(Box::new(events::parse(bytes, file_name)?)));
    }
    if file_name.to_lowercase().ends_with(".xml") {
        return Err(Error::parse("upload", "the XML export isn't needed; add the two CSV files instead"));
    }
    parse_document(bytes, team)
}

pub fn parse_document(bytes: &[u8], team: &TeamPrefix) -> Result<Document, Error> {
    let pages = pdf::extract_pages(bytes)?;
    if match_report::is_match_report(&pages) {
        Ok(Document::Match(Box::new(match_report::parse(&pages, team)?)))
    } else if players_report::is_players_report(&pages) {
        Ok(Document::Players(Box::new(players_report::parse(&pages, team)?)))
    } else {
        Err(Error::Pdf("not an InStat event export (CSV), match report or player report".to_owned()))
    }
}

/// Files for one game list the same teams and score; InStat's dates can differ by a day
/// between them.
#[must_use]
pub fn same_game(a: &Title, b: &Title) -> bool {
    a.teams == b.teams && a.score == b.score && (a.date.ordinal() - b.date.ordinal()).abs() <= 1
}

#[must_use]
pub fn describe(title: &Title) -> String {
    format!(
        "{} {} {}:{} {}",
        title.date, title.teams[0].0, title.score.0, title.score.1, title.teams[1].0
    )
}
