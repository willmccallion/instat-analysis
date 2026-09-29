//! On-disk library of uploaded reports and the games built from them.
//!
//! PDFs are kept so games can be rebuilt when the parser improves; each game is also cached
//! as JSON (tagged with [`PARSER_VERSION`]) so start-up does not re-read every PDF.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::ingest::{Document, describe, parse_document, same_game};
use crate::model::{Game, GameId, TeamPrefix};
use crate::parse::match_report::Title;
use crate::reconcile::reconcile;

/// Bump when parsing or reconciliation changes, to rebuild cached games.
pub const PARSER_VERSION: u32 = 9;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    team: TeamPrefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredGame {
    version: u32,
    /// The team the game was read for; games are rebuilt when the coach changes team.
    team: TeamPrefix,
    match_file: String,
    players_file: Option<String>,
    match_title: Title,
    game: Game,
}

#[derive(Debug, Clone)]
struct Pending {
    file: String,
    document: Document,
}

/// What happened to an uploaded file, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "message")]
pub enum AddOutcome {
    GameAdded(String),
    GameUpdated(String),
    WaitingForMatchReport(String),
}

pub struct Store {
    dir: PathBuf,
    team: Option<TeamPrefix>,
    games: Vec<StoredGame>,
    pending: Vec<Pending>,
    pub problems: Vec<String>,
}

fn slug(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

fn file_name(document: &Document) -> String {
    let title = document.title();
    let kind = match document {
        Document::Match(_) => "match",
        Document::Players(_) => "players",
    };
    format!(
        "{}_{}_{}-{}_{}_{kind}.pdf",
        title.date,
        slug(&title.teams[0].0),
        title.score.0,
        title.score.1,
        slug(&title.teams[1].0)
    )
}

impl Store {
    fn pdf_dir(&self) -> PathBuf {
        self.dir.join("pdfs")
    }

    fn game_dir(&self) -> PathBuf {
        self.dir.join("games")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    /// Opens (creating if needed) the library, loading cached games and re-reading any PDF
    /// that no current cached game accounts for. Nothing is read until a team is chosen.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let mut store = Self {
            dir: dir.to_path_buf(),
            team: None,
            games: Vec::new(),
            pending: Vec::new(),
            problems: Vec::new(),
        };
        fs::create_dir_all(store.pdf_dir())?;
        fs::create_dir_all(store.game_dir())?;
        store.team = match fs::read(store.settings_path()) {
            Ok(bytes) => Some(serde_json::from_slice::<Settings>(&bytes)?.team),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        store.load()?;
        Ok(store)
    }

    #[must_use]
    pub const fn team(&self) -> Option<&TeamPrefix> {
        self.team.as_ref()
    }

    /// Saves the coach's team and rebuilds every game for it.
    pub fn set_team(&mut self, team: TeamPrefix) -> Result<(), Error> {
        fs::write(self.settings_path(), serde_json::to_vec(&Settings { team: team.clone() })?)?;
        self.team = Some(team);
        self.games.clear();
        self.pending.clear();
        self.problems.clear();
        self.load()
    }

    /// Keeps cached games built for the current team and parser, and re-reads the rest.
    fn load(&mut self) -> Result<(), Error> {
        let Some(team) = self.team.clone() else {
            return Ok(());
        };
        for entry in fs::read_dir(self.game_dir())? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let loaded = fs::read(&path)
                    .map_err(Error::from)
                    .and_then(|bytes| serde_json::from_slice::<StoredGame>(&bytes).map_err(Error::from));
                match loaded {
                    Ok(stored) if stored.version == PARSER_VERSION && stored.team == team => {
                        self.games.push(stored);
                    }
                    _ => fs::remove_file(&path)?,
                }
            }
        }
        let mut orphans: Vec<PathBuf> = fs::read_dir(self.pdf_dir())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
                !self
                    .games
                    .iter()
                    .any(|g| g.match_file == name || g.players_file.as_deref() == Some(name.as_str()))
            })
            .collect();
        orphans.sort_by_key(|p| p.to_string_lossy().contains("_players"));
        for path in orphans {
            let bytes = fs::read(&path)?;
            if let Err(e) = self.add_pdf(&bytes) {
                let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                self.problems.push(format!("{name}: {e}"));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn games(&self) -> Vec<Game> {
        let mut games: Vec<Game> = self.games.iter().map(|g| g.game.clone()).collect();
        games.sort_by_key(|g| g.date);
        games
    }

    #[must_use]
    pub fn pending_descriptions(&self) -> Vec<String> {
        self.pending
            .iter()
            .map(|p| format!("{} (player report waiting for its match report)", describe(p.document.title())))
            .collect()
    }

    fn save_game(&mut self, stored: StoredGame) -> Result<(), Error> {
        let path = self.game_dir().join(format!("{}.json", stored.game.id.0));
        fs::write(path, serde_json::to_vec(&stored)?)?;
        self.games.retain(|g| g.game.id != stored.game.id);
        self.games.push(stored);
        Ok(())
    }

    /// Parses an uploaded PDF, keeps it, and builds or updates its game when possible.
    pub fn add_pdf(&mut self, bytes: &[u8]) -> Result<AddOutcome, Error> {
        let team = self.team.clone().ok_or(Error::NoTeam)?;
        let document = parse_document(bytes, &team)?;
        let name = file_name(&document);
        fs::write(self.pdf_dir().join(&name), bytes)?;
        match document {
            Document::Match(report) => {
                let partner_index = self
                    .pending
                    .iter()
                    .position(|p| same_game(&report.title, p.document.title()));
                let partner = partner_index.map(|i| self.pending.remove(i));
                let players = partner.as_ref().and_then(|p| match &p.document {
                    Document::Players(players) => Some(players.as_ref()),
                    Document::Match(_) => None,
                });
                let existing_players_file = self
                    .games
                    .iter()
                    .find(|g| same_game(&g.match_title, &report.title))
                    .and_then(|g| g.players_file.clone());
                let reloaded = match (players, &existing_players_file) {
                    (None, Some(file)) => match parse_document(&fs::read(self.pdf_dir().join(file))?, &team)? {
                        Document::Players(p) => Some(p),
                        Document::Match(_) => None,
                    },
                    _ => None,
                };
                let players = players.or(reloaded.as_deref());
                let game = reconcile(&report, players)?;
                let label = format!("{} vs {} {}-{}", game.date, game.opponent.0, game.goals_for, game.goals_against);
                let replaced = self.games.iter().any(|g| g.game.id == game.id);
                let players_file = partner.map(|p| p.file).or(existing_players_file);
                self.save_game(StoredGame {
                    version: PARSER_VERSION,
                    team,
                    match_file: name,
                    players_file,
                    match_title: report.title.clone(),
                    game,
                })?;
                Ok(if replaced { AddOutcome::GameUpdated(label) } else { AddOutcome::GameAdded(label) })
            }
            Document::Players(players) => {
                let Some(stored) = self
                    .games
                    .iter()
                    .find(|g| same_game(&g.match_title, &players.title))
                    .cloned()
                else {
                    let message = describe(&players.title);
                    self.pending.retain(|p| p.file != name);
                    self.pending.push(Pending {
                        file: name,
                        document: Document::Players(players),
                    });
                    return Ok(AddOutcome::WaitingForMatchReport(message));
                };
                let Document::Match(report) = parse_document(&fs::read(self.pdf_dir().join(&stored.match_file))?, &team)? else {
                    return Err(Error::Pdf("stored match report is not a match report".into()));
                };
                let game = reconcile(&report, Some(&players))?;
                let label = format!("{} vs {} {}-{}", game.date, game.opponent.0, game.goals_for, game.goals_against);
                self.save_game(StoredGame {
                    players_file: Some(name),
                    game,
                    ..stored
                })?;
                Ok(AddOutcome::GameUpdated(label))
            }
        }
    }

    /// Deletes a game and its PDFs.
    pub fn remove_game(&mut self, id: &GameId) -> Result<bool, Error> {
        let Some(index) = self.games.iter().position(|g| &g.game.id == id) else {
            return Ok(false);
        };
        let stored = self.games.remove(index);
        for file in std::iter::once(&stored.match_file).chain(stored.players_file.iter()) {
            let path = self.pdf_dir().join(file);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        let json = self.game_dir().join(format!("{}.json", id.0));
        if json.exists() {
            fs::remove_file(json)?;
        }
        Ok(true)
    }
}

/// Default library location for the platform.
#[must_use]
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("HockeyStats")
    } else if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(data).join("hockey-stats")
    } else {
        home.join(".local").join("share").join("hockey-stats")
    }
}
