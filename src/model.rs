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

/// A distance on the ice, in feet.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Feet(pub f64);

/// A spot on a standard 200 × 85 ft rink, measured from centre ice: `along` toward the
/// opponent's goal line (89 ft away), `across` toward the right of a player facing it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RinkPoint {
    pub along: Feet,
    pub across: Feet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Jersey(pub u16);

impl fmt::Display for Jersey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable identity of a player across games: normalised team + the words of their full
/// name in sorted order, so "Zach Boyd" (PDF reports) and "Boyd Zach" (event exports) match.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

impl PlayerId {
    #[must_use]
    pub fn new(team: &TeamName, full_name: &str) -> Self {
        Self(format!("{}|{}", team.0, name_key(full_name)))
    }

    /// The same id built by an older version that kept the name's word order.
    #[must_use]
    pub fn canonical(&self) -> Self {
        match self.0.split_once('|') {
            Some((team, name)) => Self(format!("{team}|{}", name_key(name))),
            None => self.clone(),
        }
    }
}

/// Upper-cased name words in sorted order.
fn name_key(name: &str) -> String {
    let mut words: Vec<String> = name.split_whitespace().map(str::to_uppercase).collect();
    words.sort();
    words.join(" ")
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

    /// Lower-case letters and digits, everything else a dash: safe in ids and file names.
    #[must_use]
    pub fn slug(&self) -> String {
        self.0.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
    }
}

/// Deterministic identifier for a game (date + teams + score).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameId(pub String);

/// Serialized as `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

impl Date {
    /// Parses `YYYY-MM-DD`.
    #[must_use]
    pub fn parse_iso(text: &str) -> Option<Self> {
        let mut parts = text.split('-');
        let year = parts.next()?.parse().ok()?;
        let month: u8 = parts.next()?.parse().ok()?;
        let day: u8 = parts.next()?.parse().ok()?;
        let valid = (1..=31).contains(&day) && (1..=12).contains(&month);
        (valid && parts.next().is_none()).then_some(Self { year, month, day })
    }
}

impl Serialize for Date {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse_iso(&text).ok_or_else(|| serde::de::Error::custom(format!("invalid date {text:?}")))
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

/// A skater's position as InStat's lines tables or the coach give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkaterPosition {
    Defence,
    Forward,
}

impl From<SkaterPosition> for Position {
    fn from(position: SkaterPosition) -> Self {
        match position {
            SkaterPosition::Defence => Self::Defence,
            SkaterPosition::Forward => Self::Forward,
        }
    }
}

/// Where a player's position in a game came from, least to most trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PositionSource {
    /// Worked out from the event export, which doesn't say.
    Guessed,
    /// InStat's match report lists them among its forward lines or defence pairs.
    Report,
    /// Set by the coach.
    Coach,
}

/// A position known before a game is built, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownPosition {
    pub position: SkaterPosition,
    pub source: PositionSource,
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

/// What a player did at one moment of a goal's build-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayKind {
    Faceoff,
    Pass,
    PassMissed,
    Entry,
    Breakout,
    DumpIn,
    DumpOut,
    Recovery,
    Loss,
    Battle,
    Hit,
    Block,
    Shot,
    Goal,
    Penalty,
}

/// One action in the build-up to a goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayEvent {
    pub time: Seconds,
    pub team: Team,
    /// Set when the player is one of ours.
    pub player: Option<PlayerId>,
    /// The player's name as shown, for either team.
    pub name: Option<String>,
    pub kind: PlayKind,
    /// What happened in words, e.g. "Faceoff won" or "Pass into the slot".
    pub detail: String,
    /// Where it happened, seen from our end (our net at −89 ft, across toward our right).
    pub at: Option<RinkPoint>,
}

/// How a goal came about, from the scoring team's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoalOrigin {
    /// Scored soon after a faceoff.
    Faceoff,
    /// Soon after winning the puck back in the attacking zone.
    Turnover,
    /// Soon after carrying, passing or dumping the puck into the zone.
    Rush,
    /// After a longer spell in the zone.
    Sustained,
}

/// The run of play from the last faceoff to a goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalPlay {
    /// The goal's time, matching its [`Goal`].
    pub time: Seconds,
    pub scored_by: Team,
    pub origin: GoalOrigin,
    /// The scoring team had a shot on goal just before.
    pub rebound: bool,
    /// When the play began: the last faceoff, or the start of the period.
    pub start: Seconds,
    pub events: Vec<PlayEvent>,
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

/// How our team's name starts in InStat reports (e.g. `SSAC`), matched case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeamPrefix(String);

impl TeamPrefix {
    /// Trims and upper-cases `text`; `None` when nothing is left.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!trimmed.is_empty()).then(|| Self(trimmed.to_uppercase()))
    }

    #[must_use]
    pub fn matches(&self, team: &TeamName) -> bool {
        team.0.to_uppercase().starts_with(&self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TeamPrefix {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| "team name is empty".to_owned())
    }
}

impl From<TeamPrefix> for String {
    fn from(team: TeamPrefix) -> Self {
        team.0
    }
}

/// Where a shot was taken from, as InStat divides the offensive zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShotZone {
    Slot,
    Center,
    RightFlank,
    LeftFlank,
    BlueLineRight,
    BlueLineCenter,
    BlueLineLeft,
}

impl ShotZone {
    pub const ALL: [Self; 7] = [
        Self::Slot,
        Self::Center,
        Self::RightFlank,
        Self::LeftFlank,
        Self::BlueLineRight,
        Self::BlueLineCenter,
        Self::BlueLineLeft,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneShots {
    pub zone: ShotZone,
    pub shots: u32,
    pub on_goal: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RinkEventKind {
    Recovery,
    Loss,
    Hit,
    BattleWon,
    BattleLost,
}

/// Something a skater did where the player report's rink maps drew it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RinkEvent {
    pub kind: RinkEventKind,
    pub at: RinkPoint,
}

/// A skater's shots on goal aimed at one part of the net, and the goals among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetShots {
    pub area: NetArea,
    pub on_goal: u32,
    pub goals: u32,
}

/// Adds `extra` into `totals` area by area, keeping areas in [`NetArea`] order.
pub fn add_net_shots(totals: &mut Vec<NetShots>, extra: &[NetShots]) {
    for e in extra {
        match totals.iter_mut().find(|t| t.area == e.area) {
            Some(t) => {
                t.on_goal += e.on_goal;
                t.goals += e.goals;
            }
            None => totals.push(*e),
        }
    }
    totals.sort_by_key(|t| t.area);
}

/// One of our shots where the match report's shooting chart drew it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartedShot {
    pub period: u32,
    /// Unknown when the marker's number is unreadable or matches no one on the roster.
    pub shooter: Option<PlayerId>,
    pub at: RinkPoint,
    pub goal: bool,
}

/// One of the opponent's shots on our net where their shooting chart drew it; `at` is
/// measured toward our goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpponentShot {
    pub period: u32,
    /// The number printed in the marker.
    pub jersey: Option<Jersey>,
    pub at: RinkPoint,
    pub goal: bool,
}

/// Adds `extra` into `totals` zone by zone, keeping zones in [`ShotZone`] order.
pub fn add_zone_shots(totals: &mut Vec<ZoneShots>, extra: &[ZoneShots]) {
    for z in extra {
        match totals.iter_mut().find(|t| t.zone == z.zone) {
            Some(t) => {
                t.shots += z.shots;
                t.on_goal += z.on_goal;
            }
            None => totals.push(*z),
        }
    }
    totals.sort_by_key(|z| z.zone);
}

/// A faceoff dot, from our point of view: left and right as a player facing the
/// opponent's net sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FaceoffSpot {
    OurZoneLeft,
    OurZoneRight,
    NeutralOurSideLeft,
    NeutralOurSideRight,
    CenterIce,
    NeutralTheirSideLeft,
    NeutralTheirSideRight,
    TheirZoneLeft,
    TheirZoneRight,
}

/// Our faceoffs at one dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotFaceoffs {
    pub spot: FaceoffSpot,
    pub won: u32,
    pub lost: u32,
}

/// Adds `extra` into `totals` dot by dot, keeping dots in [`FaceoffSpot`] order.
pub fn add_spot_faceoffs(totals: &mut Vec<SpotFaceoffs>, extra: &[SpotFaceoffs]) {
    for e in extra {
        match totals.iter_mut().find(|t| t.spot == e.spot) {
            Some(t) => {
                t.won += e.won;
                t.lost += e.lost;
            }
            None => totals.push(*e),
        }
    }
    totals.sort_by_key(|t| t.spot);
}

/// Where on the ice a puck battle happened, as InStat's challenges table splits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BattleArea {
    OwnSlot,
    BehindOwnGoal,
    OwnCorners,
    OwnBlueLine,
    NeutralZone,
    OppBlueLine,
    OppCorners,
    BehindOppGoal,
    OppSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaBattles {
    pub area: BattleArea,
    pub battles: u32,
    pub won: u32,
}

/// Adds `extra` into `totals` area by area, keeping areas in [`BattleArea`] order.
pub fn add_area_battles(totals: &mut Vec<AreaBattles>, extra: &[AreaBattles]) {
    for a in extra {
        match totals.iter_mut().find(|t| t.area == a.area) {
            Some(t) => {
                t.battles += a.battles;
                t.won += a.won;
            }
            None => totals.push(*a),
        }
    }
    totals.sort_by_key(|a| a.area);
}

/// Shots faced and saved in one category of a goalie split (a distance, a shot type, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saves<K> {
    pub kind: K,
    pub shots: u32,
    pub saves: u32,
}

/// Adds `extra` into `totals` category by category, keeping categories in `K`'s order.
pub fn add_saves<K: Copy + Ord>(totals: &mut Vec<Saves<K>>, extra: &[Saves<K>]) {
    for e in extra {
        match totals.iter_mut().find(|t| t.kind == e.kind) {
            Some(t) => {
                t.shots += e.shots;
                t.saves += e.saves;
            }
            None => totals.push(*e),
        }
    }
    totals.sort_by_key(|t| t.kind);
}

/// A split the goalie page prints as one labelled row per category.
pub trait GoaliePageRow: Copy + 'static {
    const ALL: &'static [Self];

    /// The row's label in the goalie page's Statistics block.
    fn goalie_label(self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShotDistance {
    Slot,
    CloseRange,
    MidRange,
    LongRange,
}

impl GoaliePageRow for ShotDistance {
    const ALL: &'static [Self] = &[Self::Slot, Self::CloseRange, Self::MidRange, Self::LongRange];

    fn goalie_label(self) -> &'static str {
        match self {
            Self::Slot => "From the slot",
            Self::CloseRange => "From close range",
            Self::MidRange => "From midrange",
            Self::LongRange => "From long range distance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShotType {
    Wrist,
    Snap,
    Slap,
    Deflection,
}

impl ShotType {
    /// Column header in the match report's shots table, for the types it has a column for.
    #[must_use]
    pub const fn shots_table_label(self) -> Option<&'static str> {
        match self {
            Self::Wrist => Some("Wrist shot"),
            Self::Slap => Some("Slapshot"),
            Self::Snap | Self::Deflection => None,
        }
    }
}

impl GoaliePageRow for ShotType {
    const ALL: &'static [Self] = &[Self::Wrist, Self::Snap, Self::Slap, Self::Deflection];

    fn goalie_label(self) -> &'static str {
        match self {
            Self::Wrist => "Wrist shots",
            Self::Snap => "Snap shots",
            Self::Slap => "Slap shots",
            Self::Deflection => "Deflection off the stick",
        }
    }
}

/// What the goalie was up against: alone with the shooter, screened, or a clear look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShotSituation {
    OneOnOne,
    Screened,
    CleanView,
}

impl GoaliePageRow for ShotSituation {
    const ALL: &'static [Self] = &[Self::OneOnOne, Self::Screened, Self::CleanView];

    fn goalie_label(self) -> &'static str {
        match self {
            Self::OneOnOne => "In 1 on 1 situations",
            Self::Screened => "Screen shot",
            Self::CleanView => "Clean view shot",
        }
    }
}

/// The goalie's state when the shot came.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GoalieState {
    Splitting,
    Beaten,
    Moving,
}

impl GoaliePageRow for GoalieState {
    const ALL: &'static [Self] = &[Self::Splitting, Self::Beaten, Self::Moving];

    fn goalie_label(self) -> &'static str {
        match self {
            Self::Splitting => "Goalie splitting",
            Self::Beaten => "Goalie beaten",
            Self::Moving => "Goalie in movement",
        }
    }
}

/// Where the shot met (or passed) the goalie's body; left and right are the goalie's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BodyArea {
    AboveRightShoulder,
    AboveLeftShoulder,
    AboveBlocker,
    AboveGlove,
    ChestHead,
    RightArmpit,
    LeftArmpit,
    UnderBlocker,
    UnderGlove,
    RightPad,
    LeftPad,
    BetweenLegs,
}

impl GoaliePageRow for BodyArea {
    const ALL: &'static [Self] = &[
        Self::AboveRightShoulder,
        Self::AboveLeftShoulder,
        Self::AboveBlocker,
        Self::AboveGlove,
        Self::ChestHead,
        Self::RightArmpit,
        Self::LeftArmpit,
        Self::UnderBlocker,
        Self::UnderGlove,
        Self::RightPad,
        Self::LeftPad,
        Self::BetweenLegs,
    ];

    fn goalie_label(self) -> &'static str {
        match self {
            Self::AboveRightShoulder => "Above the right shoulder",
            Self::AboveLeftShoulder => "Above the left shoulder",
            Self::AboveBlocker => "Above the blocker",
            Self::AboveGlove => "Above the glove",
            Self::ChestHead => "Chest, head",
            Self::RightArmpit => "Right armpit",
            Self::LeftArmpit => "Left armpit",
            Self::UnderBlocker => "Under the blocker",
            Self::UnderGlove => "Under the glove",
            Self::RightPad => "Right pad",
            Self::LeftPad => "Left pad",
            Self::BetweenLegs => "Between the legs",
        }
    }
}

/// A ninth of the net as InStat's net diagrams divide it, seen from the shooter's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NetArea {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Middle,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl NetArea {
    /// Rows top to bottom, each left to right.
    pub const GRID: [[Self; 3]; 3] = [
        [Self::TopLeft, Self::TopCenter, Self::TopRight],
        [Self::MiddleLeft, Self::Middle, Self::MiddleRight],
        [Self::BottomLeft, Self::BottomCenter, Self::BottomRight],
    ];
}

/// Save splits from the goalie page; each list holds the categories InStat printed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSplits {
    pub distance: Vec<Saves<ShotDistance>>,
    pub shot_type: Vec<Saves<ShotType>>,
    pub situation: Vec<Saves<ShotSituation>>,
    pub goalie_state: Vec<Saves<GoalieState>>,
    pub body_area: Vec<Saves<BodyArea>>,
    pub net_area: Vec<Saves<NetArea>>,
    pub zone: Vec<Saves<ShotZone>>,
}

impl SaveSplits {
    pub fn add(&mut self, other: &Self) {
        add_saves(&mut self.distance, &other.distance);
        add_saves(&mut self.shot_type, &other.shot_type);
        add_saves(&mut self.situation, &other.situation);
        add_saves(&mut self.goalie_state, &other.goalie_state);
        add_saves(&mut self.body_area, &other.body_area);
        add_saves(&mut self.net_area, &other.net_area);
        add_saves(&mut self.zone, &other.zone);
    }
}

/// What happened to the puck after each save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReboundControl {
    pub uncontrolled: u32,
    pub controlled: u32,
    pub frozen_after_rebound: u32,
    pub frozen_immediately: u32,
}

impl ReboundControl {
    pub const fn add(&mut self, other: &Self) {
        self.uncontrolled += other.uncontrolled;
        self.controlled += other.controlled;
        self.frozen_after_rebound += other.frozen_after_rebound;
        self.frozen_immediately += other.frozen_immediately;
    }
}

/// A count and how many of them succeeded, from cells like `"9 / 4"`: shots and shots on
/// goal, faceoffs taken and won.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub total: u32,
    pub succeeded: u32,
}

impl Tally {
    pub const fn add(&mut self, other: Self) {
        self.total += other.total;
        self.succeeded += other.succeeded;
    }
}

/// A skater's shots of one type and how many were on goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeShots {
    pub kind: ShotType,
    pub shots: Tally,
}

/// Adds `extra` into `totals` type by type, keeping types in [`ShotType`] order.
pub fn add_type_shots(totals: &mut Vec<TypeShots>, extra: &[TypeShots]) {
    for e in extra {
        match totals.iter_mut().find(|t| t.kind == e.kind) {
            Some(t) => t.shots.add(e.shots),
            None => totals.push(*e),
        }
    }
    totals.sort_by_key(|t| t.kind);
}

/// How a skater carried the puck into the offensive zone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryTypes {
    pub pass: u32,
    pub carry: u32,
    pub dump_in: u32,
}

impl EntryTypes {
    #[must_use]
    pub const fn total(self) -> u32 {
        self.pass + self.carry + self.dump_in
    }

    pub const fn add(&mut self, other: Self) {
        self.pass += other.pass;
        self.carry += other.carry;
        self.dump_in += other.dump_in;
    }
}

/// A skater's shots (attempts / on goal) by how the chance came about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotSources {
    pub power_play: Tally,
    pub short_handed: Tally,
    /// Set up in the offensive zone.
    pub positional_attack: Tally,
    /// Off a quick transition.
    pub counter_attack: Tally,
}

impl ShotSources {
    pub const fn add(&mut self, other: Self) {
        self.power_play.add(other.power_play);
        self.short_handed.add(other.short_handed);
        self.positional_attack.add(other.positional_attack);
        self.counter_attack.add(other.counter_attack);
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
    #[serde(default)]
    pub shot_zones: Vec<ZoneShots>,
    #[serde(default)]
    pub battle_areas: Vec<AreaBattles>,
    #[serde(default)]
    pub shot_sources: ShotSources,
    /// Only the types the match report's shots table has columns for.
    #[serde(default)]
    pub shot_types: Vec<TypeShots>,
    /// Taken / won.
    #[serde(default)]
    pub faceoffs_defensive_zone: Tally,
    #[serde(default)]
    pub faceoffs_offensive_zone: Tally,
    #[serde(default)]
    pub puck_losses_defensive_zone: u32,
    #[serde(default)]
    pub puck_recoveries_offensive_zone: u32,
    #[serde(default)]
    pub entry_types: EntryTypes,
    /// From the player report's rink maps.
    #[serde(default)]
    pub rink_events: Vec<RinkEvent>,
    /// From the player report's "Shots on goal" net diagram.
    #[serde(default)]
    pub net_shots: Vec<NetShots>,
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
    #[serde(default)]
    pub splits: SaveSplits,
    #[serde(default)]
    pub rebounds: Option<ReboundControl>,
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
    pub position_source: PositionSource,
    pub skater: Option<SkaterStats>,
    pub goalie: Option<GoalieStats>,
    pub shifts: Vec<Interval>,
    pub history: Vec<HistoryRow>,
    pub details: Vec<StatEntry>,
}

/// An opponent skater as InStat's distribution pages label them (number and surname).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Opponent {
    pub jersey: Option<Jersey>,
    pub surname: String,
}

/// One of our skaters against one opponent skater, from the challenge and hits
/// distribution pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matchup {
    pub player: PlayerId,
    pub opponent: Opponent,
    pub battles_won: u32,
    pub battles_lost: u32,
}

/// An opponent skater's numbers from their pages of our match report, so our players can
/// be rated against everyone on the ice, not just each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpponentSkater {
    pub opponent: Opponent,
    pub position: Position,
    pub stats: SkaterStats,
}

/// One team's side of a league game we only have the match report for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeagueSide {
    pub team: TeamName,
    pub goals: u32,
    pub summary: TeamSummary,
    pub skaters: Vec<OpponentSkater>,
    /// Their shots on the other team's net, as their shooting chart draws them.
    pub shots: Vec<OpponentShot>,
}

/// A game between two other teams in the league, from its match report alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeagueGame {
    pub id: GameId,
    pub date: Date,
    pub sides: [LeagueSide; 2],
    pub warnings: Vec<String>,
}

/// Passes from row player to column player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerMatrix {
    pub players: Vec<PlayerId>,
    /// `values[from][to]`, indexed like `players`.
    pub values: Vec<Vec<u32>>,
}

/// How a team moved the puck, counted over a game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamPlay {
    pub entries: EntryTypes,
    pub breakouts: u32,
    /// Breakouts carried or passed out under control, start to finish.
    pub controlled_breakouts: u32,
    pub dump_outs: u32,
    pub passes_to_slot: u32,
    pub puck_losses: u32,
    pub puck_losses_defensive_zone: u32,
    pub puck_recoveries: u32,
    pub puck_recoveries_offensive_zone: u32,
}

impl TeamPlay {
    pub const fn add(&mut self, other: Self) {
        self.entries.add(other.entries);
        self.breakouts += other.breakouts;
        self.controlled_breakouts += other.controlled_breakouts;
        self.dump_outs += other.dump_outs;
        self.passes_to_slot += other.passes_to_slot;
        self.puck_losses += other.puck_losses;
        self.puck_losses_defensive_zone += other.puck_losses_defensive_zone;
        self.puck_recoveries += other.puck_recoveries;
        self.puck_recoveries_offensive_zone += other.puck_recoveries_offensive_zone;
    }
}

/// The team-level numbers the analysis relies on. Possession and xG come only from
/// InStat's match report, when it is loaded.
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
    pub play: TeamPlay,
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
    /// The opponent's shots by zone, i.e. where they shot on our net.
    #[serde(default)]
    pub shot_zones_against: Vec<ZoneShots>,
    /// Our skaters against each opponent skater they met; pairs that never met are left out.
    #[serde(default)]
    pub matchups: Vec<Matchup>,
    #[serde(default)]
    pub charted_shots: Vec<ChartedShot>,
    #[serde(default)]
    pub charted_shots_against: Vec<OpponentShot>,
    #[serde(default)]
    pub faceoff_spots: Vec<SpotFaceoffs>,
    /// Every goal's build-up, in goal order.
    pub goal_plays: Vec<GoalPlay>,
    #[serde(default)]
    pub opponent_skaters: Vec<OpponentSkater>,
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
