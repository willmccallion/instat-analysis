//! Turning dropped files into documents: InStat's event exports (CSV), its match and player
//! reports (PDF), and pairing PDF reports into games.

use crate::error::Error;
use crate::model::{Game, TeamPrefix};
use crate::parse::events::{self, EventFile};
use crate::parse::match_report::{self, LeagueReport, MatchReport, Title};
use crate::parse::players_report::{self, PlayersReport};
use crate::pdf;
use crate::reconcile;

#[derive(Debug, Clone)]
pub enum Document {
    Match(Box<MatchReport>),
    Players(Box<PlayersReport>),
    /// A match report between two other teams in the league.
    League(Box<LeagueReport>),
    /// One of the two event-export files (players or team), for any game.
    Events(Box<EventFile>),
}

impl Document {
    #[must_use]
    pub fn title(&self) -> &Title {
        match self {
            Self::Match(m) => &m.title,
            Self::Players(p) => &p.title,
            Self::League(l) => &l.title,
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
        match match_report::parse(&pages, team) {
            Ok(report) => Ok(Document::Match(Box::new(report))),
            Err(Error::WrongTeam { both: false, .. }) => Ok(Document::League(Box::new(match_report::parse_league(&pages)?))),
            Err(e) => Err(e),
        }
    } else if players_report::is_players_report(&pages) {
        Ok(Document::Players(Box::new(players_report::parse(&pages, team)?)))
    } else {
        Err(Error::Pdf("not an InStat event export (CSV), match report or player report".to_owned()))
    }
}

/// The two reports for one game list the same teams and score; InStat's dates can differ
/// by a day between them.
#[must_use]
pub fn same_game(a: &Title, b: &Title) -> bool {
    a.teams == b.teams && a.score == b.score && (a.date.ordinal() - b.date.ordinal()).abs() <= 1
}

/// Pairs every match report with its player report (if present) and reconciles each game.
/// Player reports without a match report are returned as problems.
#[must_use]
pub fn build_games(documents: &[Document]) -> (Vec<Game>, Vec<String>) {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    for document in documents {
        let Document::Match(report) = document else {
            continue;
        };
        let players = documents.iter().find_map(|d| match d {
            Document::Players(p) if same_game(&report.title, &p.title) => Some(p.as_ref()),
            _ => None,
        });
        match reconcile::reconcile(report, players) {
            Ok(mut game) => {
                if players.is_none() {
                    game.warnings.push(
                        "player report not loaded: no full names, xG or recent-game history".into(),
                    );
                }
                games.push(game);
            }
            Err(e) => problems.push(format!("{}: {e}", describe(&report.title))),
        }
    }
    for document in documents {
        if let Document::Players(p) = document {
            let has_match = documents
                .iter()
                .any(|d| matches!(d, Document::Match(m) if same_game(&m.title, &p.title)));
            if !has_match {
                problems.push(format!(
                    "{}: player report loaded without its match report",
                    describe(&p.title)
                ));
            }
        }
    }
    games.sort_by_key(|g| g.date);
    (games, problems)
}

#[must_use]
pub fn describe(title: &Title) -> String {
    format!(
        "{} {} {}:{} {}",
        title.date, title.teams[0].0, title.score.0, title.score.1, title.teams[1].0
    )
}
