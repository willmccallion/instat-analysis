//! Domain types for one parsed game.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::cell::Cell;

/// Elapsed game time or a duration, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seconds(pub f64);

impl Seconds {
    #[must_use]
    pub const fn minutes(self) -> f64 {
        self.0 / 60.0
    }
}

impl std::ops::Add for Seconds {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for Seconds {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::Sub for Seconds {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl std::iter::Sum for Seconds {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|s| s.0).sum())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Jersey(pub u16);

impl fmt::Display for Jersey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable identity of a player across games: normalised team + full name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

impl PlayerId {
    #[must_use]
    pub fn new(team: &TeamName, full_name: &str) -> Self {
        Self(format!("{}|{}", team.0, normalise_name(full_name)))
    }
}

/// Upper-cased, whitespace-collapsed name used for matching.
#[must_use]
pub fn normalise_name(name: &str) -> String {
    name.split_whitespace()
        .map(str::to_uppercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Team name as printed in report headers, upper-cased.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeamName(pub String);

impl TeamName {
    #[must_use]
    pub fn new(raw: &str) -> Self {
        Self(normalise_name(raw))
    }
}

/// Deterministic identifier for a game (date + teams + score).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Parses `dd.mm.yyyy` (optionally with a trailing dot).
    #[must_use]
    pub fn parse_dotted(text: &str) -> Option<Self> {
        let mut parts = text.trim_end_matches('.').split('.');
        let day = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let year = parts.next()?.parse().ok()?;
        let valid = (1..=31).contains(&day) && (1..=12).contains(&month);
        (valid && parts.next().is_none()).then_some(Self { year, month, day })
    }

    /// Days since 0000-03-01 (proleptic Gregorian), for date arithmetic.
    #[must_use]
    pub fn ordinal(&self) -> i64 {
        let (y, m) = if self.month <= 2 {
            (i64::from(self.year) - 1, i64::from(self.month) + 9)
        } else {
            (i64::from(self.year), i64::from(self.month) - 3)
        };
        365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + i64::from(self.day) - 1
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Us (SSAC) or the opponent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    Us,
    Them,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Position {
    Defence,
    Forward,
    Goalie,
    Unknown,
}

/// Manpower situation from one team's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Strength {
    Even,
    PowerPlay,
    ShortHanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub start: Seconds,
    pub end: Seconds,
}

impl Interval {
    #[must_use]
    pub fn duration(&self) -> Seconds {
        self.end - self.start
    }

    #[must_use]
    pub fn contains(&self, t: Seconds) -> bool {
        self.start.0 <= t.0 && t.0 <= self.end.0
    }

    #[must_use]
    pub fn overlap(&self, other: &Self) -> Seconds {
        let start = self.start.0.max(other.start.0);
        let end = self.end.0.min(other.end.0);
        Seconds((end - start).max(0.0))
    }
}

/// A span during which one team had a man advantage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Advantage {
    pub interval: Interval,
    pub team: Team,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub time: Seconds,
    pub scored_by: Team,
    /// Manpower from our point of view when the goal was scored.
    pub strength: Strength,
    /// (ours, theirs) after the goal.
    pub score_after: (u32, u32),
    /// Our skaters on the ice.
    pub on_ice: Vec<PlayerId>,
}

impl Goal {
    #[must_use]
    pub fn period(&self) -> u32 {
        period_of(self.time)
    }
}

pub const PERIOD_SECONDS: f64 = 1200.0;

/// 1-based period for an elapsed game time (overtime is period 4+).
#[must_use]
pub fn period_of(t: Seconds) -> u32 {
    let index = (t.0 / PERIOD_SECONDS).floor().max(0.0);
    // Game time is bounded (< a few hours), so the cast cannot truncate meaningfully.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let period = index as u32 + 1;
    period
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnitKind {
    DefencePair,
    ForwardLine,
    /// A full five-skater even-strength unit ("basic line").
    FullUnit,
    PowerPlay,
    PenaltyKill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvenStrengthUnitStats {
    pub plus_minus: i64,
    pub goals_for: u32,
    pub goals_against: u32,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub penalties_drawn: u32,
    pub penalties_taken: u32,
    pub possession_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecialTeamsUnitStats {
    pub shifts: u32,
    /// Goals scored (power play) or conceded (penalty kill).
    pub goals: u32,
    /// Shots for (power play) or against (penalty kill).
    pub shots: u32,
    pub shots_on_goal: u32,
    pub time_in_offensive_zone: Seconds,
    pub faceoffs_won: u32,
    pub opponent_breakouts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnitStats {
    EvenStrength(EvenStrengthUnitStats),
    SpecialTeams(SpecialTeamsUnitStats),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub kind: UnitKind,
    pub players: Vec<PlayerId>,
    pub toi: Seconds,
    pub stats: UnitStats,
}

/// A labelled cell from a report table, kept for display ("all stats" views).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatEntry {
    pub group: String,
    pub label: String,
    pub value: CellValue,
}

/// Serializable wrapper so stored games do not depend on [`Cell`]'s serde shape changing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellValue {
    pub text: String,
    /// Primary number for sorting/charting, when the cell has one.
    pub number: Option<f64>,
}

impl CellValue {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let number = match Cell::parse(text) {
            Cell::Empty => Some(0.0),
            Cell::Int(n) => Some(n as f64),
            Cell::Decimal(d) | Cell::Percent(d) => Some(d),
            Cell::Clock(s) | Cell::ClockShare(s, _) => Some(f64::from(s)),
            Cell::Ratio(a, _)
            | Cell::Triple(a, _, _)
            | Cell::CountShare(a, _)
            | Cell::Pair(a, _) => Some(f64::from(a)),
            Cell::Text(_) => None,
        };
        let shown = if text.trim().is_empty() {
            "—"
        } else {
            text.trim()
        };
        Self {
            text: shown.to_owned(),
            number,
        }
    }
}

/// The per-game skater numbers the analysis relies on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkaterStats {
    pub instat_index: Option<f64>,
    pub goals: u32,
    pub assists: u32,
    pub plus_minus: i64,
    pub toi: Seconds,
    pub shifts: u32,
    pub pp_toi: Seconds,
    pub sh_toi: Seconds,
    pub penalty_minutes: Seconds,
    pub shots: u32,
    pub shots_on_goal: u32,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub hits: u32,
    pub hits_against: u32,
    pub faceoffs: u32,
    pub faceoffs_won: u32,
    pub blocked_shots: u32,
    pub puck_battles: u32,
    pub puck_battles_won: u32,
    pub puck_losses: u32,
    pub puck_recoveries: u32,
    pub entries: u32,
    pub passes: u32,
    pub xg: Option<f64>,
    pub on_ice_xg_for: Option<f64>,
    pub on_ice_xg_against: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoalieStats {
    pub instat_index: Option<f64>,
    pub toi: Seconds,
    pub shots_against: u32,
    pub saves: u32,
    pub goals_against: u32,
    pub even_strength: Option<(u32, u32)>,
    pub short_handed: Option<(u32, u32)>,
}

/// One row of a player's "comparison with recent games" table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryRow {
    /// `dd.mm` as printed; the year is inferred during reconciliation.
    pub date: Date,
    pub opponent: String,
    pub instat_index: Option<f64>,
    pub toi: Seconds,
    pub kind: HistoryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryKind {
    Skater {
        goals: u32,
        assists: u32,
        shots: u32,
        shots_on_goal: u32,
        plus_minus: i64,
    },
    Goalie {
        shots_against: u32,
        saves: u32,
        goals_against: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub surname: String,
    pub jersey: Option<Jersey>,
    pub position: Position,
    /// InStat's grouping on the time-distribution page, e.g. "FIRST LINE".
    pub group: Option<String>,
    pub skater: Option<SkaterStats>,
    pub goalie: Option<GoalieStats>,
    pub shifts: Vec<Interval>,
    pub history: Vec<HistoryRow>,
    pub details: Vec<StatEntry>,
}

/// Passes from row player to column player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerMatrix {
    pub players: Vec<PlayerId>,
    /// `values[from][to]`, indexed like `players`.
    pub values: Vec<Vec<u32>>,
}

/// The team-level numbers the analysis relies on (from the TEAMS STATS page).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamSummary {
    pub shots: u32,
    pub shots_on_goal: u32,
    /// Shots / on goal / goals per regulation period.
    pub shots_by_period: Vec<(u32, u32, u32)>,
    pub even_strength_shots: (u32, u32, u32),
    pub power_play_shots: (u32, u32, u32),
    pub scoring_chances: (u32, u32, u32),
    pub xg: Option<f64>,
    pub blocked_shots: u32,
    pub faceoffs_won: u32,
    pub faceoffs_won_by_zone: [u32; 3],
    pub puck_battles_won: u32,
    pub penalties: u32,
    pub penalty_time: Seconds,
    pub power_plays: u32,
    pub power_play_goals: u32,
    pub power_play_time: Seconds,
    pub possession_time: Seconds,
    pub possession_pct: Option<f64>,
    pub possession_pct_by_period: Vec<f64>,
    pub hits: u32,
}

/// One game from our point of view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub date: Date,
    pub team: TeamName,
    pub opponent: TeamName,
    pub goals_for: u32,
    pub goals_against: u32,
    pub players: Vec<Player>,
    pub units: Vec<Unit>,
    pub passes: Option<PlayerMatrix>,
    pub goals: Vec<Goal>,
    pub advantages: Vec<Advantage>,
    pub summary: TeamSummary,
    pub opponent_summary: TeamSummary,
    /// Every team-level stat for (us, them), for display.
    pub team_stats: Vec<TeamStatRow>,
    /// Regulation plus any overtime.
    pub length: Seconds,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamStatRow {
    pub group: String,
    pub label: String,
    pub ours: CellValue,
    pub theirs: CellValue,
}

impl Game {
    /// Our manpower state at time `t`.
    #[must_use]
    pub fn strength_at(&self, t: Seconds) -> Strength {
        self.advantages
            .iter()
            .find(|a| a.interval.start.0 < t.0 && t.0 <= a.interval.end.0)
            .map_or(Strength::Even, |a| match a.team {
                Team::Us => Strength::PowerPlay,
                Team::Them => Strength::ShortHanded,
            })
    }

    #[must_use]
    pub fn player(&self, id: &PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| &p.id == id)
    }
}
