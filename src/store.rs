//! On-disk library of uploaded files and the games built from them.
//!
//! Every file is kept so games can be rebuilt when the reader improves; each game is also
//! cached as JSON (tagged with [`PARSER_VERSION`]) so start-up does not re-read every file.
//! A game is built from its event export, with any PDF reports adding InStat's own numbers.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::analysis::rating_setup::RatingWeights;
use crate::analysis::targets::PlayerTarget;
use crate::enrich::{add_reports, listed_positions};
use crate::error::Error;
use crate::events::{EventGame, build_game, build_league_game};
use crate::ingest::{Document, describe, parse_upload, same_game};
use crate::model::{Game, GameId, KnownPosition, LeagueGame, PlayerId, PositionSource, SkaterPosition, TeamPrefix};
use crate::parse::events::{EventFile, EventFileKind};
use crate::parse::match_report::{Title, our_index};

/// Bump when parsing or reconciliation changes, to rebuild cached games.
pub const PARSER_VERSION: u32 = 17;

/// Players' positions from one source: the coach, or InStat's match report.
pub type Positions = BTreeMap<PlayerId, SkaterPosition>;

/// The coach setting one player's position, or with `None` handing it back to the app.
#[derive(Debug, Clone, Deserialize)]
pub struct PositionChange {
    pub player: PlayerId,
    pub position: Option<SkaterPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    team: TeamPrefix,
    #[serde(default)]
    rating_weights: RatingWeights,
    #[serde(default)]
    player_targets: Vec<PlayerTarget>,
    #[serde(default)]
    player_positions: Positions,
}

/// Which of InStat's downloads a file is, in the order a game is best assembled from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FileKind {
    PlayersExport,
    TeamExport,
    MatchReport,
    PlayersReport,
}

impl FileKind {
    const fn of(document: &Document) -> Self {
        match document {
            Document::Events(file) if matches!(file.kind, EventFileKind::Players) => Self::PlayersExport,
            Document::Events(_) => Self::TeamExport,
            Document::Match(_) => Self::MatchReport,
            Document::Players(_) => Self::PlayersReport,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::PlayersExport => "players CSV",
            Self::TeamExport => "team CSV",
            Self::MatchReport => "match report",
            Self::PlayersReport => "player report",
        }
    }
}

/// The library's file names for one game.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GameFiles {
    players_export: Option<String>,
    team_export: Option<String>,
    match_report: Option<String>,
    players_report: Option<String>,
}

impl GameFiles {
    const fn slot(&mut self, kind: FileKind) -> &mut Option<String> {
        match kind {
            FileKind::PlayersExport => &mut self.players_export,
            FileKind::TeamExport => &mut self.team_export,
            FileKind::MatchReport => &mut self.match_report,
            FileKind::PlayersReport => &mut self.players_report,
        }
    }

    const fn has(&self, kind: FileKind) -> bool {
        match kind {
            FileKind::PlayersExport => self.players_export.is_some(),
            FileKind::TeamExport => self.team_export.is_some(),
            FileKind::MatchReport => self.match_report.is_some(),
            FileKind::PlayersReport => self.players_report.is_some(),
        }
    }

    fn names(&self) -> impl Iterator<Item = &String> {
        [&self.players_export, &self.team_export, &self.match_report, &self.players_report].into_iter().flatten()
    }
}

/// What a game's files add up to so far.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum Built {
    Ours(Box<Game>),
    League(Box<LeagueGame>),
    /// Not enough files yet; says which is missing.
    Waiting(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    version: u32,
    /// The team the game was read for; games are rebuilt when the coach changes team.
    team: TeamPrefix,
    title: Title,
    files: GameFiles,
    built: Built,
    /// The coach's positions an [`Built::Ours`] game was built with.
    #[serde(default)]
    coach_positions: Positions,
    /// Where the game's match report puts our players.
    #[serde(default)]
    report_positions: Positions,
}

impl Entry {
    /// Whether this entry was built the way the current reader, team and positions would.
    fn is_current(&self, team: &TeamPrefix, coach_positions: &Positions) -> bool {
        let positions_match = !matches!(self.built, Built::Ours(_)) || &self.coach_positions == coach_positions;
        self.version == PARSER_VERSION && &self.team == team && positions_match
    }

    fn game_id(&self) -> Option<&GameId> {
        match &self.built {
            Built::Ours(game) => Some(&game.id),
            Built::League(game) => Some(&game.id),
            Built::Waiting(_) => None,
        }
    }

    fn json_name(&self) -> String {
        format!("{}.json", title_key(&self.title))
    }
}

/// The documents read back for one game.
struct Documents {
    players_export: Option<EventFile>,
    team_export: Option<EventFile>,
    match_report: Option<Document>,
    players_report: Option<Document>,
}

/// What happened to an uploaded file, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "message")]
pub enum AddOutcome {
    GameAdded(String),
    GameUpdated(String),
    LeagueGameAdded(String),
    LeagueGameUpdated(String),
    /// Kept, but the game needs another file before it can be shown.
    Waiting(String),
    /// This file (or another copy of it) is already loaded, so nothing was added.
    AlreadyLoaded(String),
}

pub struct Store {
    dir: PathBuf,
    team: Option<TeamPrefix>,
    rating_weights: RatingWeights,
    player_targets: Vec<PlayerTarget>,
    coach_positions: Positions,
    entries: Vec<Entry>,
    pub problems: Vec<String>,
}

fn title_key(title: &Title) -> String {
    format!("{}_{}_{}-{}_{}", title.date, title.teams[0].slug(), title.score.0, title.score.1, title.teams[1].slug())
}

/// The name a file is kept under. Event exports keep InStat's naming, since their date and
/// score are read from the name.
fn stored_name(document: &Document) -> String {
    let title = document.title();
    match FileKind::of(document) {
        kind @ (FileKind::PlayersExport | FileKind::TeamExport) => format!(
            "{} {} _ {} {} {:02}.{:02}.{}{}.csv",
            title.teams[0].0,
            title.score.0,
            title.score.1,
            title.teams[1].0,
            title.date.day,
            title.date.month,
            title.date.year,
            if kind == FileKind::TeamExport { "-2" } else { "" }
        ),
        FileKind::MatchReport => format!("{}_match.pdf", title_key(title)),
        FileKind::PlayersReport => format!("{}_players.pdf", title_key(title)),
    }
}

fn game_label(game: &Game) -> String {
    format!("{} vs {} {}-{}", game.date, game.opponent.0, game.goals_for, game.goals_against)
}

/// Positions for building a game: the coach's over the match report's.
fn known_positions(coach: &Positions, report: &Positions) -> HashMap<PlayerId, KnownPosition> {
    let known = |positions: &Positions, source: PositionSource| {
        positions.iter().map(move |(id, position)| (id.clone(), KnownPosition { position: *position, source })).collect::<Vec<_>>()
    };
    known(report, PositionSource::Report).into_iter().chain(known(coach, PositionSource::Coach)).collect()
}

/// Gives `game` the units and positions its export makes with `known` positions. The PDF
/// reports add nothing to either, so `game` keeps everything they gave it.
fn reposition(game: &mut Game, export: &EventGame<'_>, team: &TeamPrefix, known: &HashMap<PlayerId, KnownPosition>) -> Result<(), Error> {
    let rebuilt = build_game(export, team, known)?;
    game.units = rebuilt.units;
    for player in &mut game.players {
        if let Some(fresh) = rebuilt.players.iter().find(|p| p.id == player.id) {
            player.position = fresh.position;
            player.position_source = fresh.position_source;
        }
    }
    Ok(())
}

/// What a game's files make up, and where its match report puts our players.
struct Assembled {
    built: Built,
    report_positions: Positions,
}

/// The game `documents` make up for `team`, with `coach_positions` over InStat's over the
/// app's own guess.
fn build(team: &TeamPrefix, title: &Title, documents: &Documents, coach_positions: &Positions) -> Result<Assembled, Error> {
    let only = |built: Built| Assembled { built, report_positions: Positions::new() };
    let ours = match our_index(title, team) {
        Ok(_) => true,
        Err(Error::WrongTeam { both: false, .. }) => false,
        Err(e) => return Err(e),
    };
    let match_report = match &documents.match_report {
        Some(Document::Match(report)) => Some(report.as_ref()),
        _ => None,
    };
    let players_report = match &documents.players_report {
        Some(Document::Players(report)) => Some(report.as_ref()),
        _ => None,
    };
    let Some(players) = &documents.players_export else {
        return Ok(only(Built::Waiting("add its players CSV (the one with shifts)".to_owned())));
    };
    let export = EventGame { players, team: documents.team_export.as_ref() };
    if !ours {
        return Ok(only(Built::League(Box::new(build_league_game(&export)?))));
    }
    let mut game = build_game(&export, team, &known_positions(coach_positions, &Positions::new()))?;
    add_reports(&mut game, match_report, players_report);
    let report_positions: Positions = match_report.map(|report| listed_positions(&game, report).into_iter().collect()).unwrap_or_default();
    if !report_positions.is_empty() {
        reposition(&mut game, &export, team, &known_positions(coach_positions, &report_positions))?;
    }
    Ok(Assembled { built: Built::Ours(Box::new(game)), report_positions })
}

impl Store {
    fn pdf_dir(&self) -> PathBuf {
        self.dir.join("pdfs")
    }

    fn export_dir(&self) -> PathBuf {
        self.dir.join("exports")
    }

    fn library_dir(&self) -> PathBuf {
        self.dir.join("library")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    fn file_path(&self, name: &str) -> PathBuf {
        if Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")) {
            self.export_dir().join(name)
        } else {
            self.pdf_dir().join(name)
        }
    }

    /// Opens (creating if needed) the library, loading cached games and re-reading any file
    /// that no current cached game accounts for. Nothing is read until a team is chosen.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let mut store = Self {
            dir: dir.to_path_buf(),
            team: None,
            rating_weights: RatingWeights::default(),
            player_targets: Vec::new(),
            coach_positions: Positions::new(),
            entries: Vec::new(),
            problems: Vec::new(),
        };
        fs::create_dir_all(store.pdf_dir())?;
        fs::create_dir_all(store.export_dir())?;
        fs::create_dir_all(store.library_dir())?;
        store.remove_old_caches()?;
        match fs::read(store.settings_path()) {
            Ok(bytes) => {
                let settings = serde_json::from_slice::<Settings>(&bytes)?;
                store.team = Some(settings.team);
                store.rating_weights = settings.rating_weights;
                store.player_targets = settings
                    .player_targets
                    .into_iter()
                    .map(|t| PlayerTarget { player: t.player.canonical(), ..t })
                    .collect();
                store.coach_positions = settings.player_positions.into_iter().map(|(id, p)| (id.canonical(), p)).collect();
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        store.load()?;
        Ok(store)
    }

    /// Before version 16 games were cached in `games/` and `league/`; the files they were
    /// built from are still in `pdfs/` and are re-read from there.
    fn remove_old_caches(&self) -> Result<(), Error> {
        for old in ["games", "league"] {
            let dir = self.dir.join(old);
            if !dir.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    fs::remove_file(path)?;
                }
            }
            if fs::read_dir(&dir)?.next().is_none() {
                fs::remove_dir(dir)?;
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn team(&self) -> Option<&TeamPrefix> {
        self.team.as_ref()
    }

    #[must_use]
    pub const fn rating_weights(&self) -> &RatingWeights {
        &self.rating_weights
    }

    #[must_use]
    pub fn player_targets(&self) -> &[PlayerTarget] {
        &self.player_targets
    }

    #[must_use]
    pub const fn coach_positions(&self) -> &Positions {
        &self.coach_positions
    }

    /// Saves the coach's position for a player and rebuilds our games with it.
    pub fn set_player_position(&mut self, change: &PositionChange) -> Result<(), Error> {
        let team = self.team.clone().ok_or(Error::NoTeam)?;
        let player = change.player.canonical();
        match change.position {
            Some(position) => self.coach_positions.insert(player, position),
            None => self.coach_positions.remove(&player),
        };
        self.save_settings(&team)?;
        self.rebuild_our_games(&team)
    }

    /// Reapplies positions to our games from their event exports alone; quick, since the
    /// PDF reports, by far the slowest to read, add nothing to units or positions.
    fn rebuild_our_games(&mut self, team: &TeamPrefix) -> Result<(), Error> {
        for index in 0..self.entries.len() {
            let entry = &self.entries[index];
            let Built::Ours(cached) = &entry.built else {
                continue;
            };
            let read = |name: &Option<String>| -> Result<Option<EventFile>, Error> {
                match name.as_ref().map(|n| parse_upload(&fs::read(self.file_path(n))?, n, team)).transpose()? {
                    Some(Document::Events(file)) => Ok(Some(*file)),
                    Some(_) => Err(Error::parse("library", format!("{} isn't an event export", describe(&entry.title)))),
                    None => Ok(None),
                }
            };
            let players = read(&entry.files.players_export)?.ok_or_else(|| Error::parse("library", format!("{} has no players CSV", describe(&entry.title))))?;
            let team_export = read(&entry.files.team_export)?;
            let mut game = cached.as_ref().clone();
            let known = known_positions(&self.coach_positions, &entry.report_positions);
            reposition(&mut game, &EventGame { players: &players, team: team_export.as_ref() }, team, &known)?;
            let updated = Entry { built: Built::Ours(Box::new(game)), coach_positions: self.coach_positions.clone(), ..entry.clone() };
            fs::write(self.library_dir().join(updated.json_name()), serde_json::to_vec(&updated)?)?;
            self.entries[index] = updated;
        }
        Ok(())
    }

    /// Saves the coach's player targets.
    pub fn set_player_targets(&mut self, targets: Vec<PlayerTarget>) -> Result<(), Error> {
        let team = self.team.clone().ok_or(Error::NoTeam)?;
        self.player_targets = targets;
        self.save_settings(&team)
    }

    fn save_settings(&self, team: &TeamPrefix) -> Result<(), Error> {
        let settings = Settings {
            team: team.clone(),
            rating_weights: self.rating_weights.clone(),
            player_targets: self.player_targets.clone(),
            player_positions: self.coach_positions.clone(),
        };
        fs::write(self.settings_path(), serde_json::to_vec(&settings)?)?;
        Ok(())
    }

    /// Saves the coach's player-rating weights.
    pub fn set_rating_weights(&mut self, weights: RatingWeights) -> Result<(), Error> {
        let team = self.team.clone().ok_or(Error::NoTeam)?;
        self.rating_weights = weights;
        self.save_settings(&team)
    }

    /// Saves the coach's team and rebuilds every game for it.
    pub fn set_team(&mut self, team: TeamPrefix) -> Result<(), Error> {
        self.save_settings(&team)?;
        self.team = Some(team);
        self.entries.clear();
        self.problems.clear();
        self.load()
    }

    /// Keeps cached games built for the current team and reader, and re-reads the rest.
    fn load(&mut self) -> Result<(), Error> {
        let Some(team) = self.team.clone() else {
            return Ok(());
        };
        for entry in fs::read_dir(self.library_dir())? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let loaded = fs::read(&path)
                    .map_err(Error::from)
                    .and_then(|bytes| serde_json::from_slice::<Entry>(&bytes).map_err(Error::from));
                match loaded {
                    Ok(entry) if entry.is_current(&team, &self.coach_positions) => self.entries.push(entry),
                    _ => fs::remove_file(&path)?,
                }
            }
        }
        let mut orphans: Vec<(FileKind, String, Vec<u8>)> = Vec::new();
        for dir in [self.export_dir(), self.pdf_dir()] {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
                if self.entries.iter().any(|e| e.files.names().any(|f| *f == name)) {
                    continue;
                }
                let bytes = fs::read(&path)?;
                match parse_upload(&bytes, &name, &team) {
                    Ok(document) => orphans.push((FileKind::of(&document), name, bytes)),
                    Err(e) => self.problems.push(format!("{name}: {e}")),
                }
            }
        }
        orphans.sort_by_key(|(kind, _, _)| *kind);
        for (_, name, bytes) in orphans {
            if let Err(e) = self.add_file(&bytes, &name) {
                self.problems.push(format!("{name}: {e}"));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn games(&self) -> Vec<Game> {
        let mut games: Vec<Game> = self
            .entries
            .iter()
            .filter_map(|e| match &e.built {
                Built::Ours(game) => Some(game.as_ref().clone()),
                Built::League(_) | Built::Waiting(_) => None,
            })
            .collect();
        games.sort_by_key(|g| g.date);
        games
    }

    #[must_use]
    pub fn league_games(&self) -> Vec<LeagueGame> {
        let mut games: Vec<LeagueGame> = self
            .entries
            .iter()
            .filter_map(|e| match &e.built {
                Built::League(game) => Some(game.as_ref().clone()),
                Built::Ours(_) | Built::Waiting(_) => None,
            })
            .collect();
        games.sort_by_key(|g| g.date);
        games
    }

    #[must_use]
    pub fn pending_descriptions(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match &e.built {
                Built::Waiting(missing) => Some(format!("{}: {missing}", describe(&e.title))),
                Built::Ours(_) | Built::League(_) => None,
            })
            .collect()
    }

    fn read_documents(&self, team: &TeamPrefix, files: &GameFiles) -> Result<Documents, Error> {
        let read = |name: &Option<String>| -> Result<Option<Document>, Error> {
            name.as_ref().map(|n| parse_upload(&fs::read(self.file_path(n))?, n, team)).transpose()
        };
        let export = |document: Option<Document>| match document {
            Some(Document::Events(file)) => Some(*file),
            _ => None,
        };
        Ok(Documents {
            players_export: export(read(&files.players_export)?),
            team_export: export(read(&files.team_export)?),
            match_report: read(&files.match_report)?,
            players_report: read(&files.players_report)?,
        })
    }

    fn save_entry(&mut self, entry: Entry, replacing: Option<usize>) -> Result<(), Error> {
        if let Some(old) = replacing.map(|i| self.entries.remove(i))
            && old.json_name() != entry.json_name()
        {
            let path = self.library_dir().join(old.json_name());
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        fs::write(self.library_dir().join(entry.json_name()), serde_json::to_vec(&entry)?)?;
        self.entries.push(entry);
        Ok(())
    }

    /// Reads an uploaded file, keeps it, and builds or updates its game when it can.
    pub fn add_file(&mut self, bytes: &[u8], file_name: &str) -> Result<AddOutcome, Error> {
        let team = self.team.clone().ok_or(Error::NoTeam)?;
        let document = parse_upload(bytes, file_name, &team)?;
        let kind = FileKind::of(&document);
        let index = self.entries.iter().position(|e| same_game(&e.title, document.title()));
        let mut files = index.map(|i| self.entries[i].files.clone()).unwrap_or_default();
        if files.has(kind) {
            return Ok(AddOutcome::AlreadyLoaded(format!("{} for {}", kind.label(), describe(document.title()))));
        }
        let name = stored_name(&document);
        *files.slot(kind) = Some(name.clone());
        let title = match (kind, index) {
            (FileKind::MatchReport | FileKind::PlayersReport, Some(i)) => self.entries[i].title.clone(),
            _ => document.title().clone(),
        };
        let path = self.file_path(&name);
        fs::write(&path, bytes)?;
        let Assembled { built, report_positions } = self
            .read_documents(&team, &files)
            .and_then(|documents| build(&team, &title, &documents, &self.coach_positions))
            .inspect_err(|_| {
                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("could not remove {name}: {e}");
                }
            })?;
        let was_built = index.is_some_and(|i| !matches!(self.entries[i].built, Built::Waiting(_)));
        let outcome = match &built {
            Built::Ours(game) if was_built => AddOutcome::GameUpdated(game_label(game)),
            Built::Ours(game) => AddOutcome::GameAdded(game_label(game)),
            Built::League(_) if was_built => AddOutcome::LeagueGameUpdated(describe(&title)),
            Built::League(_) => AddOutcome::LeagueGameAdded(describe(&title)),
            Built::Waiting(missing) => AddOutcome::Waiting(format!("{}: {missing}", describe(&title))),
        };
        let coach_positions = if matches!(built, Built::Ours(_)) { self.coach_positions.clone() } else { Positions::new() };
        self.save_entry(Entry { version: PARSER_VERSION, team, title, files, built, coach_positions, report_positions }, index)?;
        Ok(outcome)
    }

    /// Deletes a game (ours or a league game) and its files.
    pub fn remove_game(&mut self, id: &GameId) -> Result<bool, Error> {
        let Some(index) = self.entries.iter().position(|e| e.game_id() == Some(id)) else {
            return Ok(false);
        };
        let entry = self.entries.remove(index);
        for name in entry.files.names() {
            let path = self.file_path(name);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        let json = self.library_dir().join(entry.json_name());
        if json.exists() {
            fs::remove_file(json)?;
        }
        Ok(true)
    }
}

/// Default library location for the platform.
#[must_use]
pub fn default_dir() -> PathBuf {
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        return PathBuf::from(local).join("HockeyStats");
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("HockeyStats")
    } else if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(data).join("hockey-stats")
    } else {
        home.join(".local").join("share").join("hockey-stats")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Position;

    const HEADER: &str = "ID,start,end,duration,pos_x,pos_y,player,team,action,half";

    fn players_csv(teams: [&str; 2]) -> String {
        let mut lines = vec![HEADER.to_owned()];
        for (team, names) in teams.iter().zip([["McDavid Connor", "Skinner Stuart"], ["Crosby Sidney", "Jarry Tristan"]]) {
            for name in names {
                lines.push(format!("0,0,1200,1200,,,{name},{team},All shifts,1"));
            }
        }
        lines.push(format!("0,594,606,12,,,Jarry Tristan,{},Shots against,1", teams[1]));
        lines.push(format!("0,594,606,12,,,Skinner Stuart,{},Shots against,1", teams[0]));
        lines.join("\n")
    }

    fn team_csv(teams: [&str; 2]) -> String {
        format!("{HEADER}\n0,10,22,12,,,,{},OZ play,1\n0,10,22,12,,,,{},OZ play,1", teams[0], teams[1])
    }

    fn library(name: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("hockey-store-{}-{name}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let mut store = Store::open(&dir).unwrap();
        store.set_team(TeamPrefix::parse("Team One").unwrap()).unwrap();
        (dir, store)
    }

    const OURS: [&str; 2] = ["Team One", "Team Two"];
    const GAME: &str = "Team One 0 _ 0 Team Two 26.09.2026";

    #[test]
    fn the_team_csv_waits_for_the_players_csv() {
        let (dir, mut store) = library("waits");

        let first = store.add_file(team_csv(OURS).as_bytes(), &format!("{GAME}-2.csv")).unwrap();
        let second = store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME}.csv")).unwrap();

        assert!(matches!(first, AddOutcome::Waiting(_)), "{first:?}");
        assert!(matches!(second, AddOutcome::GameAdded(_)), "{second:?}");
        assert_eq!(store.games().len(), 1);
        assert!(store.pending_descriptions().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_second_copy_of_a_file_is_skipped() {
        let (dir, mut store) = library("copy");
        store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME}.csv")).unwrap();

        let again = store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME} (1).csv")).unwrap();

        assert!(matches!(again, AddOutcome::AlreadyLoaded(_)), "{again:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn games_are_rebuilt_from_kept_files_when_the_cache_is_gone() {
        let (dir, mut store) = library("rebuild");
        store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME}.csv")).unwrap();
        store.add_file(team_csv(OURS).as_bytes(), &format!("{GAME}-2.csv")).unwrap();
        fs::remove_dir_all(dir.join("library")).unwrap();

        let reopened = Store::open(&dir).unwrap();

        assert_eq!(reopened.games().len(), 1);
        assert!(reopened.problems.is_empty(), "{:?}", reopened.problems);
        assert!(!reopened.games()[0].warnings.iter().any(|w| w.contains("team file")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn another_teams_game_goes_to_the_league() {
        let (dir, mut store) = library("league");
        let others = ["Team Three", "Team Four"];

        let outcome = store.add_file(players_csv(others).as_bytes(), "Team Three 0 _ 0 Team Four 26.09.2026.csv").unwrap();

        assert!(matches!(outcome, AddOutcome::LeagueGameAdded(_)), "{outcome:?}");
        assert_eq!((store.games().len(), store.league_games().len()), (0, 1));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_coach_position_wins_and_survives_reopening_until_cleared() {
        let (dir, mut store) = library("position");
        store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME}.csv")).unwrap();
        let skater = store.games()[0].players.iter().find(|p| p.skater.is_some()).unwrap().id.clone();
        let position_of = |store: &Store| {
            let player = store.games()[0].players.iter().find(|p| p.id == skater).unwrap().clone();
            (player.position, player.position_source)
        };

        store.set_player_position(&PositionChange { player: skater.clone(), position: Some(SkaterPosition::Defence) }).unwrap();
        let reopened = Store::open(&dir).unwrap();
        store.set_player_position(&PositionChange { player: skater.clone(), position: None }).unwrap();

        assert_eq!(position_of(&reopened), (Position::Defence, PositionSource::Coach));
        assert_eq!(position_of(&store), (Position::Forward, PositionSource::Guessed));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removing_a_game_deletes_its_files() {
        let (dir, mut store) = library("remove");
        store.add_file(players_csv(OURS).as_bytes(), &format!("{GAME}.csv")).unwrap();
        let id = store.games()[0].id.clone();

        let removed = store.remove_game(&id).unwrap();

        assert!(removed);
        assert_eq!(fs::read_dir(dir.join("exports")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(dir.join("library")).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
