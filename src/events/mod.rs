//! Games built from InStat's event export: every player action with its time and position,
//! and every shift, for both teams.
//!
//! The export times everything in video seconds; the running game clock is rebuilt from the
//! shifts (players are on the ice only while it runs). Positions are converted to the app's
//! rink feet in the acting team's frame.

mod plays;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::Error;
use crate::model::{
    Advantage, AreaBattles, BattleArea, CellValue, ChartedShot, EntryTypes, EvenStrengthUnitStats, FaceoffSpot, Feet, Game, GameId,
    Goal, GoalieStats, Interval, LeagueGame, LeagueSide, Matchup, Opponent, OpponentShot, OpponentSkater, PERIOD_SECONDS, Player,
    PlayerId, PlayerMatrix, Position, RinkEvent, RinkEventKind, RinkPoint, SaveSplits, Seconds, ShotSources, ShotZone, SkaterStats,
    SpecialTeamsUnitStats, SpotFaceoffs, Strength, Tally, Team, TeamName, TeamPlay, TeamPrefix, TeamStatRow, TeamSummary, Unit,
    UnitKind, UnitStats, ZoneShots, add_area_battles, add_spot_faceoffs, add_zone_shots,
};
use crate::parse::events::{Action, EventFile, EventFileKind, Span, VideoTime};
use crate::parse::match_report::{Title, our_index};

const SECTION: &str = "event export";
const METRES_PER_FOOT: f64 = 0.3048;
const RINK_LENGTH_M: f64 = 60.96;
const RINK_WIDTH_M: f64 = 25.92;
const REGULATION_PERIODS: u32 = 3;
/// A regulation period whose shifts add up to within this of 20:00 is stretched to exactly
/// 20:00; the export's whole-second times drift by a few seconds a period.
const PERIOD_DRIFT: f64 = 30.0;
/// Clock stretches closer than this are one: a shift or power play carrying on over a whistle.
const CONTINUOUS: f64 = 1.0;
/// Goals are placed this long before their time when finding who was on the ice, since the
/// whistle ends the shifts at the goal's moment.
const ON_ICE_PROBE: f64 = 0.25;
/// A pass's receiver is the next teammate to act within this many video seconds.
const PASS_WINDOW: f64 = 10.0;
/// Faceoffs per game that mark a skater as a centre for the position guess.
const CENTRE_FACEOFFS: usize = 3;
/// The export doesn't give penalty lengths; minors are by far the most common.
const PENALTY_SECONDS: f64 = 120.0;

/// Converts the export's video time to the game clock.
struct Clock {
    /// Per period, the merged stretches when someone was on the ice (the clock running),
    /// and how much each video second counts on the game clock.
    periods: BTreeMap<u32, (Vec<(VideoTime, VideoTime)>, f64)>,
}

impl Clock {
    fn new(spans: &[Span]) -> Self {
        let mut stretches: BTreeMap<u32, Vec<(VideoTime, VideoTime)>> = BTreeMap::new();
        for s in spans.iter().filter(|s| s.name == "All shifts") {
            stretches.entry(s.period).or_default().push((s.start, s.end));
        }
        let periods = stretches
            .into_iter()
            .map(|(period, mut list)| {
                list.sort_unstable();
                let mut merged: Vec<(VideoTime, VideoTime)> = Vec::new();
                for (a, b) in list {
                    match merged.last_mut() {
                        Some(last) if a <= last.1 => last.1 = last.1.max(b),
                        _ => merged.push((a, b)),
                    }
                }
                let run: f64 = merged.iter().map(|(a, b)| b.seconds() - a.seconds()).sum();
                let near_regulation = period <= REGULATION_PERIODS && (run - PERIOD_SECONDS).abs() <= PERIOD_DRIFT;
                (period, (merged, if near_regulation { PERIOD_SECONDS / run } else { 1.0 }))
            })
            .collect();
        Self { periods }
    }

    fn period_length(&self, period: u32) -> f64 {
        self.periods
            .get(&period)
            .map_or(0.0, |(stretches, scale)| scale * stretches.iter().map(|(a, b)| b.seconds() - a.seconds()).sum::<f64>())
    }

    /// Regulation periods whose shifts don't add up to about 20:00, so the video is likely
    /// missing part of them.
    fn irregular_periods(&self) -> Vec<u32> {
        (1..=REGULATION_PERIODS).filter(|p| (self.period_length(*p) - PERIOD_SECONDS).abs() > 1.0).collect()
    }

    /// Game seconds at video time `t` in `period`.
    fn at(&self, period: u32, t: VideoTime) -> Seconds {
        let before: f64 = (1..period).map(|p| if p <= REGULATION_PERIODS { PERIOD_SECONDS } else { self.period_length(p) }).sum();
        let run = self.periods.get(&period).map_or(0.0, |(stretches, scale)| {
            scale * stretches.iter().map(|&(a, b)| (t.min(b).seconds() - a.seconds()).max(0.0)).sum::<f64>()
        });
        Seconds(before + run)
    }

    fn length(&self) -> Seconds {
        let last = self.periods.keys().last().copied().unwrap_or(REGULATION_PERIODS).max(REGULATION_PERIODS);
        self.at(last, VideoTime::END)
    }

    fn interval(&self, span: &Span) -> Interval {
        Interval { start: self.at(span.period, span.start), end: self.at(span.period, span.end) }
    }

    fn action(&self, action: &Action) -> Seconds {
        self.at(action.period, action.at)
    }
}

/// Sorted intervals with those that touch (within [`CONTINUOUS`]) joined.
fn joined(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by(|a, b| a.start.0.total_cmp(&b.start.0));
    let mut result: Vec<Interval> = Vec::new();
    for i in intervals {
        match result.last_mut() {
            Some(last) if i.start.0 - last.end.0 < CONTINUOUS => last.end = Seconds(last.end.0.max(i.end.0)),
            _ => result.push(i),
        }
    }
    result
}

/// Rink feet in the acting team's frame (their target net at +89 ft, across toward their right).
fn point((x, y): (f64, f64)) -> RinkPoint {
    RinkPoint {
        along: Feet(x / METRES_PER_FOOT - 100.0),
        across: Feet(RINK_WIDTH_M / 2.0 / METRES_PER_FOOT - y / METRES_PER_FOOT),
    }
}

/// The same spot seen from the other end of the rink.
fn turned((x, y): (f64, f64)) -> (f64, f64) {
    (RINK_LENGTH_M - x, RINK_WIDTH_M - y)
}

/// A contested action's spot in this team's frame. Faceoffs and puck battles give both
/// teams' rows one position, in the winner's frame.
fn contested_point(position: (f64, f64), won: bool) -> RinkPoint {
    point(if won { position } else { turned(position) })
}

/// "Boyd Zach" (the export's order) as "Zach Boyd".
fn display_name(export_name: &str) -> String {
    let mut words: Vec<&str> = export_name.split_whitespace().collect();
    if let Some(first) = words.pop() {
        words.insert(0, first);
    }
    words.join(" ")
}

fn surname(export_name: &str) -> String {
    export_name.split_whitespace().next().unwrap_or_default().to_owned()
}

/// InStat's zone for a shot. The boundaries are fitted to the zones its reports give for
/// shots the export positions (95% agree; the rest sit on a boundary).
fn shot_zone(at: RinkPoint) -> ShotZone {
    let out = 89.0 - at.along.0;
    let wide = at.across.0.abs();
    let left = at.across.0 < 0.0;
    if out >= 37.0 {
        return match (wide < 16.0, left) {
            (true, _) => ShotZone::BlueLineCenter,
            (false, true) => ShotZone::BlueLineLeft,
            (false, false) => ShotZone::BlueLineRight,
        };
    }
    if (0.0..18.0).contains(&out) && wide < 0.6f64.mul_add(out, 5.0) {
        return ShotZone::Slot;
    }
    if out >= 0.0 && wide < 0.8f64.mul_add(out, 4.5) {
        return ShotZone::Center;
    }
    if left { ShotZone::LeftFlank } else { ShotZone::RightFlank }
}

/// InStat's area for a puck battle, from its spot in the battling team's frame. The
/// boundaries are fitted to the areas its reports give for battles the export positions
/// (97% agree): the blue lines, the end-zone dots' line, a strip 9 ft either side of the net,
/// and 3 ft past the goal line for battles behind it.
fn battle_area(at: RinkPoint) -> BattleArea {
    let (along, net_front) = (at.along.0, at.across.0.abs() <= 9.0);
    match along {
        a if a < -92.0 => BattleArea::BehindOwnGoal,
        a if a < -69.0 && net_front => BattleArea::OwnSlot,
        a if a < -69.0 => BattleArea::OwnCorners,
        a if a < -25.0 => BattleArea::OwnBlueLine,
        a if a <= 25.0 => BattleArea::NeutralZone,
        a if a <= 69.0 => BattleArea::OppBlueLine,
        a if a <= 92.0 && net_front => BattleArea::OppSlot,
        a if a <= 92.0 => BattleArea::OppCorners,
        _ => BattleArea::BehindOppGoal,
    }
}

fn count<'a>(actions: impl IntoIterator<Item = &'a &'a Action>, name: &str) -> u32 {
    u32::try_from(actions.into_iter().filter(|a| a.name == name).count()).unwrap_or(u32::MAX)
}

/// One team's rows and the other team's actions.
struct TeamRows<'a> {
    actions: Vec<&'a Action>,
    spans: Vec<&'a Span>,
    /// This team's actions and spans in the team file, if it was loaded.
    file_actions: Vec<&'a Action>,
    file_spans: Vec<&'a Span>,
    opponent_actions: Vec<&'a Action>,
}

impl<'a> TeamRows<'a> {
    fn new(game: &EventGame<'a>, name: &TeamName) -> Self {
        Self {
            actions: game.players.actions.iter().filter(|a| &a.team == name).collect(),
            spans: game.players.spans.iter().filter(|s| &s.team == name).collect(),
            file_actions: game.team.map(|f| f.actions.iter().filter(|a| &a.team == name).collect()).unwrap_or_default(),
            file_spans: game.team.map(|f| f.spans.iter().filter(|s| &s.team == name).collect()).unwrap_or_default(),
            opponent_actions: game.players.actions.iter().filter(|a| &a.team != name).collect(),
        }
    }

    fn count(&self, name: &str) -> u32 {
        count(&self.actions, name)
    }

    /// Like [`Self::count`], but from the team file when it is loaded; it also lists
    /// actions no single player is credited with.
    fn team_count(&self, name: &str) -> u32 {
        if self.file_actions.is_empty() { self.count(name) } else { count(&self.file_actions, name) }
    }

    /// Moments at which this team has an action called `name`.
    fn moments(&self, name: &str) -> BTreeSet<VideoTime> {
        self.actions.iter().filter(|a| a.name == name).map(|a| a.at).collect()
    }

    fn players(&self) -> Vec<&'a str> {
        let mut names: Vec<&str> = self.spans.iter().filter_map(|s| s.player.as_deref()).chain(self.actions.iter().filter_map(|a| a.player.as_deref())).collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// Man advantages, from the team file's power-play periods or, without it, from the
/// players' power-play shifts. `true` marks this team's power plays.
fn advantages(team: &TeamRows<'_>, other: &TeamRows<'_>, clock: &Clock) -> Vec<(Interval, bool)> {
    let from = |spans: &[&Span], name: &str| joined(spans.iter().filter(|s| s.name == name).map(|s| clock.interval(s)).collect());
    let (ours, theirs) = if team.file_spans.is_empty() && other.file_spans.is_empty() {
        (from(&team.spans, "Power play shifts"), from(&other.spans, "Power play shifts"))
    } else {
        (from(&team.file_spans, "Power play"), from(&other.file_spans, "Power play"))
    };
    let mut all: Vec<(Interval, bool)> = ours.into_iter().map(|i| (i, true)).chain(theirs.into_iter().map(|i| (i, false))).collect();
    all.sort_by(|a, b| a.0.start.0.total_cmp(&b.0.start.0));
    all
}

fn strength_at(advantages: &[(Interval, bool)], t: Seconds) -> Strength {
    advantages
        .iter()
        .find(|(i, _)| i.start.0 < t.0 && t.0 <= i.end.0)
        .map_or(Strength::Even, |(_, ours)| if *ours { Strength::PowerPlay } else { Strength::ShortHanded })
}

/// Each player's shifts on the game clock, a shift carrying on over a whistle counted once.
fn shifts(team: &TeamRows<'_>, clock: &Clock) -> HashMap<String, Vec<Interval>> {
    let mut pieces: BTreeMap<(&str, u32), Vec<Interval>> = BTreeMap::new();
    for s in team.spans.iter().filter(|s| s.name == "All shifts") {
        if let Some(name) = &s.player {
            pieces.entry((name.as_str(), s.period)).or_default().push(clock.interval(s));
        }
    }
    let mut result: HashMap<String, Vec<Interval>> = HashMap::new();
    for ((name, _), list) in pieces {
        result.entry(name.to_owned()).or_default().extend(joined(list));
    }
    result
}

fn on_shift(shifts: &[Interval], t: Seconds) -> bool {
    shifts.iter().any(|s| s.start.0 < t.0 && t.0 <= s.end.0)
}

fn on_ice(shifts: &HashMap<String, Vec<Interval>>, t: Seconds) -> Vec<&str> {
    let mut names: Vec<&str> = shifts.iter().filter(|(_, list)| on_shift(list, t)).map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    names
}

fn goalies(team: &TeamRows<'_>) -> BTreeSet<String> {
    team.actions
        .iter()
        .filter(|a| matches!(a.name.as_str(), "Saves" | "Shots against" | "Goals against"))
        .filter_map(|a| a.player.clone())
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    let n = u32::try_from(values.len()).unwrap_or(u32::MAX);
    if n == 0 { 0.0 } else { values.iter().sum::<f64>() / f64::from(n) }
}

fn standardised(values: &[f64]) -> Vec<f64> {
    let m = mean(values);
    let sd = mean(&values.iter().map(|v| (v - m).powi(2)).collect::<Vec<_>>()).sqrt();
    values.iter().map(|v| if sd > 0.0 { (v - m) / sd } else { 0.0 }).collect()
}

fn overlap(a: &[Interval], b: &[Interval]) -> f64 {
    a.iter().flat_map(|x| b.iter().map(move |y| x.overlap(y).0)).sum()
}

/// Forward or defence for every skater. Centres take faceoffs; defencemen play deeper and
/// spend the most even-strength time beside the centres, since a pair plays behind every line.
fn guess_positions(team: &TeamRows<'_>, shifts: &HashMap<String, Vec<Interval>>, skaters: &[&str]) -> HashMap<String, Position> {
    let faceoffs = |name: &str| team.actions.iter().filter(|a| a.name == "Faceoffs" && a.player.as_deref() == Some(name)).count();
    let centres: BTreeSet<&str> = skaters.iter().copied().filter(|n| faceoffs(n) >= CENTRE_FACEOFFS).collect();
    let empty = Vec::new();
    let shifts_of = |n: &str| shifts.get(n).unwrap_or(&empty);
    let share_with_centres = |n: &str| {
        let with_all: f64 = skaters.iter().filter(|o| **o != n).map(|o| overlap(shifts_of(n), shifts_of(o))).sum();
        let with_centres: f64 = centres.iter().filter(|o| **o != n).map(|o| overlap(shifts_of(n), shifts_of(o))).sum();
        if with_all > 0.0 { with_centres / with_all } else { 0.0 }
    };
    let depth = |n: &str| {
        let xs: Vec<f64> = team
            .actions
            .iter()
            .filter(|a| a.player.as_deref() == Some(n) && !a.name.starts_with("Faceoffs"))
            .filter_map(|a| a.position.map(|(x, _)| x))
            .collect();
        mean(&xs)
    };
    let shares = standardised(&skaters.iter().map(|n| share_with_centres(n)).collect::<Vec<_>>());
    let depths = standardised(&skaters.iter().map(|n| depth(n)).collect::<Vec<_>>());
    let mut scored: Vec<(&str, f64)> = skaters.iter().copied().zip(shares.iter().zip(&depths).map(|(s, d)| s - d)).collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let defence = (skaters.len() + 1) / 3;
    scored
        .iter()
        .enumerate()
        .map(|(i, (n, _))| ((*n).to_owned(), if i < defence && !centres.contains(n) { Position::Defence } else { Position::Forward }))
        .collect()
}

/// Of `actions` called `total`, how many share their moment with one called `succeeded`.
fn tally(actions: &[&Action], total: &str, succeeded: &str) -> Tally {
    let hits: BTreeSet<VideoTime> = actions.iter().filter(|a| a.name == succeeded).map(|a| a.at).collect();
    let all: Vec<&&Action> = actions.iter().filter(|a| a.name == total).collect();
    Tally {
        total: u32::try_from(all.len()).unwrap_or(u32::MAX),
        succeeded: u32::try_from(all.iter().filter(|a| hits.contains(&a.at)).count()).unwrap_or(u32::MAX),
    }
}

/// A goal from one team's point of view.
#[derive(Debug, Clone)]
struct GoalEvent {
    time: Seconds,
    /// Scored by this team.
    scored: bool,
    /// Manpower from this team's point of view.
    strength: Strength,
    /// This team's skaters on the ice (goalies excluded).
    on_ice: Vec<String>,
    /// This team's score, then the other team's, after the goal.
    score_after: (u32, u32),
}

/// Everything about one team's game that its players' numbers are built from.
struct Side<'a> {
    team: TeamRows<'a>,
    shifts: HashMap<String, Vec<Interval>>,
    goalies: BTreeSet<String>,
    advantages: Vec<(Interval, bool)>,
    /// Even-strength shot attempts by this team and by the other team, on the game clock.
    attempts_for: Vec<Seconds>,
    attempts_against: Vec<Seconds>,
    goals: Vec<GoalEvent>,
    /// (passer, receiver) → completed passes, receivers inferred.
    passes: BTreeMap<(String, String), u32>,
}

fn even_attempts(actions: &[&Action], clock: &Clock, advantages: &[(Interval, bool)]) -> Vec<Seconds> {
    actions
        .iter()
        .filter(|a| a.name == "Shots")
        .map(|a| clock.action(a))
        .filter(|t| strength_at(advantages, *t) == Strength::Even)
        .collect()
}

fn goal_events(team: &TeamRows<'_>, clock: &Clock, advantages: &[(Interval, bool)], shifts: &HashMap<String, Vec<Interval>>, goalies: &BTreeSet<String>) -> Vec<GoalEvent> {
    let mut goals: Vec<(Seconds, bool)> = team
        .actions
        .iter()
        .filter(|a| a.name == "Goals")
        .map(|a| (clock.action(a), true))
        .chain(team.opponent_actions.iter().filter(|a| a.name == "Goals").map(|a| (clock.action(a), false)))
        .collect();
    goals.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));
    let (mut ours, mut theirs) = (0, 0);
    goals
        .into_iter()
        .map(|(time, scored)| {
            if scored { ours += 1 } else { theirs += 1 }
            let probe = Seconds(time.0 - ON_ICE_PROBE);
            GoalEvent {
                time,
                scored,
                strength: strength_at(advantages, probe),
                on_ice: on_ice(shifts, probe).into_iter().filter(|n| !goalies.contains(*n)).map(str::to_owned).collect(),
                score_after: (ours, theirs),
            }
        })
        .collect()
}

/// The receiver of each completed pass: the next teammate to act within a few seconds.
fn pass_receivers(team: &TeamRows<'_>) -> BTreeMap<(String, String), u32> {
    let mut ordered: Vec<&Action> = team.actions.iter().copied().filter(|a| a.player.is_some()).collect();
    ordered.sort_by_key(|a| a.at);
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    for (i, pass) in ordered.iter().enumerate().filter(|(_, a)| a.name == "Accurate passes") {
        let Some(from) = &pass.player else {
            continue;
        };
        let receiver = ordered[i + 1..]
            .iter()
            .take_while(|a| a.at.seconds() - pass.at.seconds() <= PASS_WINDOW)
            .find_map(|a| a.player.as_ref().filter(|p| *p != from));
        if let Some(to) = receiver {
            *counts.entry((from.clone(), to.clone())).or_default() += 1;
        }
    }
    counts
}

fn side<'a>(game: &EventGame<'a>, name: &TeamName, other: &TeamName, clock: &Clock) -> Side<'a> {
    let team = TeamRows::new(game, name);
    let opponent = TeamRows::new(game, other);
    let shifts = shifts(&team, clock);
    let goalies = goalies(&team);
    let advantages = advantages(&team, &opponent, clock);
    let attempts_for = even_attempts(&team.actions, clock, &advantages);
    let attempts_against = even_attempts(&opponent.actions, clock, &advantages);
    let goals = goal_events(&team, clock, &advantages, &shifts, &goalies);
    let passes = pass_receivers(&team);
    Side { team, shifts, goalies, advantages, attempts_for, attempts_against, goals, passes }
}

fn rink_event(action: &Action) -> Option<RinkEvent> {
    let position = action.position?;
    let (kind, at) = match action.name.as_str() {
        "Puck recoveries" => (RinkEventKind::Recovery, point(position)),
        "Puck losses" => (RinkEventKind::Loss, point(position)),
        "Hits" => (RinkEventKind::Hit, point(position)),
        "Puck battles won" => (RinkEventKind::BattleWon, contested_point(position, true)),
        "Puck battles lost" => (RinkEventKind::BattleLost, contested_point(position, false)),
        _ => return None,
    };
    Some(RinkEvent { kind, at })
}

fn shot_zones(mine: &[&Action]) -> Vec<ZoneShots> {
    let on_goal: BTreeSet<VideoTime> = mine.iter().filter(|a| a.name == "Shots on goal").map(|a| a.at).collect();
    let mut zones = Vec::new();
    for shot in mine.iter().filter(|a| a.name == "Shots") {
        if let Some(at) = shot.position {
            add_zone_shots(&mut zones, &[ZoneShots { zone: shot_zone(point(at)), shots: 1, on_goal: u32::from(on_goal.contains(&shot.at)) }]);
        }
    }
    zones
}

fn battle_areas(mine: &[&Action]) -> Vec<AreaBattles> {
    let won: BTreeSet<VideoTime> = mine.iter().filter(|a| a.name == "Puck battles won").map(|a| a.at).collect();
    let mut areas = Vec::new();
    for battle in mine.iter().filter(|a| a.name == "Puck battles") {
        if let Some(position) = battle.position {
            let we_won = won.contains(&battle.at);
            add_area_battles(&mut areas, &[AreaBattles { area: battle_area(contested_point(position, we_won)), battles: 1, won: u32::from(we_won) }]);
        }
    }
    areas
}

fn plus_minus(name: &str, goals: &[GoalEvent]) -> i64 {
    goals
        .iter()
        .filter(|g| g.on_ice.iter().any(|n| n == name))
        .map(|g| match (g.scored, g.strength) {
            (true, Strength::PowerPlay) | (false, Strength::ShortHanded) => 0,
            (true, _) => 1,
            (false, _) => -1,
        })
        .sum()
}

/// Everything a skater did, from their rows.
fn skater_stats(name: &str, side: &Side<'_>) -> SkaterStats {
    let mine: Vec<&Action> = side.team.actions.iter().copied().filter(|a| a.player.as_deref() == Some(name)).collect();
    let my_spans: Vec<&Span> = side.team.spans.iter().copied().filter(|s| s.player.as_deref() == Some(name)).collect();
    let n = |action: &str| count(&mine, action);
    let time_in = |span: &str| my_spans.iter().filter(|s| s.name == span).map(|s| s.duration()).sum();
    let empty = Vec::new();
    let own = side.shifts.get(name).unwrap_or(&empty);
    let on_ice_count = |times: &[Seconds]| u32::try_from(times.iter().filter(|t| on_shift(own, **t)).count()).unwrap_or(u32::MAX);
    SkaterStats {
        instat_index: None,
        goals: n("Goals"),
        assists: n("Assists"),
        plus_minus: plus_minus(name, &side.goals),
        toi: time_in("All shifts"),
        shifts: u32::try_from(own.len()).unwrap_or(u32::MAX),
        pp_toi: time_in("Power play shifts"),
        sh_toi: time_in("Penalty kill shifts"),
        penalty_minutes: Seconds(f64::from(n("Penalties")) * PENALTY_SECONDS),
        shots: n("Shots"),
        shots_on_goal: n("Shots on goal"),
        corsi_for: on_ice_count(&side.attempts_for),
        corsi_against: on_ice_count(&side.attempts_against),
        hits: n("Hits"),
        hits_against: 0,
        faceoffs: n("Faceoffs"),
        faceoffs_won: n("Faceoffs won"),
        blocked_shots: n("Shots blocking"),
        puck_battles: n("Puck battles"),
        puck_battles_won: n("Puck battles won"),
        puck_losses: n("Puck losses"),
        puck_recoveries: n("Puck recoveries"),
        entries: n("Entries"),
        passes: n("Accurate passes"),
        xg: None,
        on_ice_xg_for: None,
        on_ice_xg_against: None,
        shot_zones: shot_zones(&mine),
        battle_areas: battle_areas(&mine),
        shot_sources: ShotSources {
            power_play: tally(&mine, "Power play shots", "Shots on goal"),
            short_handed: tally(&mine, "Short-handed shots", "Shots on goal"),
            ..ShotSources::default()
        },
        shot_types: Vec::new(),
        faceoffs_defensive_zone: tally(&mine, "Faceoffs in DZ", "Faceoffs won"),
        faceoffs_offensive_zone: tally(&mine, "Faceoffs in OZ", "Faceoffs won"),
        puck_losses_defensive_zone: n("Puck losses in DZ"),
        puck_recoveries_offensive_zone: n("Puck recoveries in OZ"),
        entry_types: EntryTypes { pass: n("Entries via pass"), carry: n("Entries via stickhandling"), dump_in: n("Entries via dump in") },
        rink_events: mine.iter().filter_map(|a| rink_event(a)).collect(),
        net_shots: Vec::new(),
    }
}

fn goalie_stats(name: &str, side: &Side<'_>, clock: &Clock) -> GoalieStats {
    let mine: Vec<&Action> = side.team.actions.iter().copied().filter(|a| a.player.as_deref() == Some(name)).collect();
    let saved_at: BTreeSet<VideoTime> = mine.iter().filter(|a| a.name == "Saves").map(|a| a.at).collect();
    let by_strength = |strength: Strength| {
        let faced: Vec<&&Action> = mine.iter().filter(|a| a.name == "Shots against" && strength_at(&side.advantages, clock.action(a)) == strength).collect();
        let stopped = faced.iter().filter(|f| saved_at.contains(&f.at)).count();
        (u32::try_from(faced.len()).unwrap_or(u32::MAX), u32::try_from(stopped).unwrap_or(u32::MAX))
    };
    GoalieStats {
        instat_index: None,
        toi: side.team.spans.iter().filter(|s| s.name == "All shifts" && s.player.as_deref() == Some(name)).map(|s| s.duration()).sum(),
        shots_against: count(&mine, "Shots against"),
        saves: count(&mine, "Saves"),
        goals_against: count(&mine, "Goals against"),
        even_strength: Some(by_strength(Strength::Even)),
        short_handed: Some(by_strength(Strength::ShortHanded)),
        splits: SaveSplits::default(),
        rebounds: None,
    }
}

/// Who played what, for building units.
struct Roster<'a> {
    ids: &'a HashMap<String, PlayerId>,
    positions: &'a HashMap<String, Position>,
}

impl Roster<'_> {
    fn ids_of(&self, names: &[&str], position: Option<Position>) -> Vec<PlayerId> {
        let mut ids: Vec<PlayerId> = names
            .iter()
            .filter(|n| position.is_none_or(|p| self.positions.get(**n) == Some(&p)))
            .filter_map(|n| self.ids.get(*n).cloned())
            .collect();
        ids.sort();
        ids
    }
}

/// The units a group of skaters on the ice together makes up.
fn groups_on_ice(skaters: &[&str], strength: Strength, roster: &Roster<'_>) -> Vec<(UnitKind, Vec<PlayerId>)> {
    match strength {
        Strength::Even => {
            let (d, f) = (roster.ids_of(skaters, Some(Position::Defence)), roster.ids_of(skaters, Some(Position::Forward)));
            let mut groups = Vec::new();
            if d.len() == 2 && f.len() == 3 {
                groups.push((UnitKind::FullUnit, roster.ids_of(skaters, None)));
            }
            if d.len() == 2 {
                groups.push((UnitKind::DefencePair, d));
            }
            if f.len() == 3 {
                groups.push((UnitKind::ForwardLine, f));
            }
            groups
        }
        Strength::PowerPlay => vec![(UnitKind::PowerPlay, roster.ids_of(skaters, None))],
        Strength::ShortHanded => vec![(UnitKind::PenaltyKill, roster.ids_of(skaters, None))],
    }
}

#[derive(Default)]
struct UnitTotals {
    seconds: f64,
    goals_for: u32,
    goals_against: u32,
    corsi_for: u32,
    corsi_against: u32,
}

impl UnitTotals {
    fn stats(&self, kind: UnitKind) -> UnitStats {
        match kind {
            UnitKind::PowerPlay | UnitKind::PenaltyKill => UnitStats::SpecialTeams(SpecialTeamsUnitStats {
                shifts: 0,
                goals: if kind == UnitKind::PowerPlay { self.goals_for } else { self.goals_against },
                shots: 0,
                shots_on_goal: 0,
                time_in_offensive_zone: Seconds(0.0),
                faceoffs_won: 0,
                opponent_breakouts: 0,
            }),
            UnitKind::DefencePair | UnitKind::ForwardLine | UnitKind::FullUnit => UnitStats::EvenStrength(EvenStrengthUnitStats {
                plus_minus: i64::from(self.goals_for) - i64::from(self.goals_against),
                goals_for: self.goals_for,
                goals_against: self.goals_against,
                corsi_for: self.corsi_for,
                corsi_against: self.corsi_against,
                penalties_drawn: 0,
                penalties_taken: 0,
                possession_pct: None,
            }),
        }
    }
}

/// Line combinations as stints of one exact group on the ice, with their shot attempts.
fn units(side: &Side<'_>, roster: &Roster<'_>, clock: &Clock) -> Vec<Unit> {
    let mut cuts: Vec<f64> = side.shifts.values().flatten().flat_map(|s| [s.start.0, s.end.0]).collect();
    cuts.extend(side.advantages.iter().flat_map(|(i, _)| [i.start.0, i.end.0]));
    cuts.push(clock.length().0);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    let within = |t: f64, a: f64, b: f64| a < t && t <= b;
    let count_within = |times: &[Seconds], a: f64, b: f64| u32::try_from(times.iter().filter(|t| within(t.0, a, b)).count()).unwrap_or(u32::MAX);
    let goals_within = |scored: bool, a: f64, b: f64| u32::try_from(side.goals.iter().filter(|g| g.scored == scored && within(g.time.0 - ON_ICE_PROBE, a, b)).count()).unwrap_or(u32::MAX);
    let mut totals: BTreeMap<(UnitKind, Vec<PlayerId>), UnitTotals> = BTreeMap::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let middle = Seconds(f64::midpoint(a, b));
        let skaters: Vec<&str> = on_ice(&side.shifts, middle).into_iter().filter(|n| !side.goalies.contains(*n)).collect();
        for key in groups_on_ice(&skaters, strength_at(&side.advantages, middle), roster) {
            if key.1.is_empty() {
                continue;
            }
            let entry = totals.entry(key).or_default();
            entry.seconds += b - a;
            entry.goals_for += goals_within(true, a, b);
            entry.goals_against += goals_within(false, a, b);
            entry.corsi_for += count_within(&side.attempts_for, a, b);
            entry.corsi_against += count_within(&side.attempts_against, a, b);
        }
    }
    totals
        .into_iter()
        .map(|((kind, players), t)| Unit { stats: t.stats(kind), kind, players, toi: Seconds(t.seconds) })
        .collect()
}

/// Head-to-head puck battles: our winner or loser and their opposite number at the same moment.
fn matchups(side: &Side<'_>, ids: &HashMap<String, PlayerId>) -> Vec<Matchup> {
    let mut result: BTreeMap<(PlayerId, String), Matchup> = BTreeMap::new();
    for a in side.team.actions.iter().filter(|a| a.name == "Puck battles won" || a.name == "Puck battles lost") {
        let Some(id) = a.player.as_deref().and_then(|p| ids.get(p)) else {
            continue;
        };
        let won = a.name == "Puck battles won";
        let opposite = if won { "Puck battles lost" } else { "Puck battles won" };
        let Some(opponent) = side.team.opponent_actions.iter().find(|o| o.name == opposite && o.at == a.at).and_then(|o| o.player.as_deref()) else {
            continue;
        };
        let entry = result.entry((id.clone(), opponent.to_owned())).or_insert_with(|| Matchup {
            player: id.clone(),
            opponent: Opponent { jersey: None, surname: display_name(opponent) },
            battles_won: 0,
            battles_lost: 0,
        });
        if won { entry.battles_won += 1 } else { entry.battles_lost += 1 }
    }
    result.into_values().collect()
}

fn summary(side: &Side<'_>, clock: &Clock) -> TeamSummary {
    let team = &side.team;
    let (on_goal, goals) = (team.moments("Shots on goal"), team.moments("Goals"));
    let shots: Vec<&&Action> = team.actions.iter().filter(|a| a.name == "Shots").collect();
    let triple = |moments: &[VideoTime]| {
        let n = |set: &BTreeSet<VideoTime>| u32::try_from(moments.iter().filter(|t| set.contains(t)).count()).unwrap_or(u32::MAX);
        (u32::try_from(moments.len()).unwrap_or(u32::MAX), n(&on_goal), n(&goals))
    };
    let shots_where = |keep: &dyn Fn(&Action) -> bool| triple(&shots.iter().filter(|a| keep(a)).map(|a| a.at).collect::<Vec<_>>());
    let strength_of = |a: &Action| strength_at(&side.advantages, clock.action(a));
    let chances: BTreeSet<VideoTime> = team.file_actions.iter().filter(|a| a.name == "Scoring chances").map(|a| a.at).collect();
    let won = team.moments("Faceoffs won");
    let faceoffs_won_in = |zone: &str| {
        let name = format!("Faceoffs in {zone}");
        u32::try_from(team.actions.iter().filter(|a| a.name == name && won.contains(&a.at)).count()).unwrap_or(u32::MAX)
    };
    let power_plays: Vec<&Interval> = side.advantages.iter().filter(|(_, ours)| *ours).map(|(i, _)| i).collect();
    TeamSummary {
        shots: team.count("Shots"),
        shots_on_goal: team.count("Shots on goal"),
        shots_by_period: (1..=REGULATION_PERIODS).map(|p| shots_where(&|a: &Action| a.period == p)).collect(),
        even_strength_shots: shots_where(&|a: &Action| strength_of(a) == Strength::Even),
        power_play_shots: shots_where(&|a: &Action| strength_of(a) == Strength::PowerPlay),
        scoring_chances: triple(&chances.into_iter().collect::<Vec<_>>()),
        xg: None,
        blocked_shots: team.count("Blocked shots"),
        faceoffs_won: team.count("Faceoffs won"),
        faceoffs_won_by_zone: [faceoffs_won_in("DZ"), faceoffs_won_in("NZ"), faceoffs_won_in("OZ")],
        puck_battles_won: team.count("Puck battles won"),
        penalties: team.count("Penalties"),
        penalty_time: Seconds(f64::from(team.count("Penalties")) * PENALTY_SECONDS),
        power_plays: u32::try_from(power_plays.len()).unwrap_or(u32::MAX),
        power_play_goals: u32::try_from(side.goals.iter().filter(|g| g.scored && g.strength == Strength::PowerPlay).count()).unwrap_or(u32::MAX),
        power_play_time: power_plays.iter().map(|i| i.duration()).sum(),
        possession_time: Seconds(0.0),
        possession_pct: None,
        possession_pct_by_period: Vec::new(),
        hits: team.count("Hits"),
        play: team_play(team),
    }
}

fn team_play(team: &TeamRows<'_>) -> TeamPlay {
    TeamPlay {
        entries: EntryTypes {
            pass: team.count("Entries via pass"),
            carry: team.count("Entries via stickhandling"),
            dump_in: team.count("Entries via dump in"),
        },
        breakouts: team.count("Breakouts"),
        controlled_breakouts: u32::try_from(team.file_spans.iter().filter(|s| s.name == "Controlled breakouts").count()).unwrap_or(u32::MAX),
        dump_outs: team.count("Dump outs"),
        passes_to_slot: team.count("Passes to the slot"),
        puck_losses: team.count("Puck losses"),
        puck_losses_defensive_zone: team.count("Puck losses in DZ"),
        puck_recoveries: team.team_count("Puck recoveries"),
        puck_recoveries_offensive_zone: team.team_count("Puck recoveries in OZ"),
    }
}

/// The nine faceoff dots in rink feet (along, across), in our frame.
const DOTS: [(FaceoffSpot, f64, f64); 9] = [
    (FaceoffSpot::OurZoneLeft, -69.0, -22.0),
    (FaceoffSpot::OurZoneRight, -69.0, 22.0),
    (FaceoffSpot::NeutralOurSideLeft, -20.0, -22.0),
    (FaceoffSpot::NeutralOurSideRight, -20.0, 22.0),
    (FaceoffSpot::CenterIce, 0.0, 0.0),
    (FaceoffSpot::NeutralTheirSideLeft, 20.0, -22.0),
    (FaceoffSpot::NeutralTheirSideRight, 20.0, 22.0),
    (FaceoffSpot::TheirZoneLeft, 69.0, -22.0),
    (FaceoffSpot::TheirZoneRight, 69.0, 22.0),
];

fn nearest_dot(at: RinkPoint) -> FaceoffSpot {
    let distance = |&(_, along, across): &(FaceoffSpot, f64, f64)| (along - at.along.0).hypot(across - at.across.0);
    DOTS.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).map_or(FaceoffSpot::CenterIce, |(spot, _, _)| *spot)
}

/// Faceoffs won and lost at each dot.
fn faceoff_spots(side: &Side<'_>) -> Vec<SpotFaceoffs> {
    let won = side.team.moments("Faceoffs won");
    let mut spots = Vec::new();
    for faceoff in side.team.actions.iter().filter(|a| a.name == "Faceoffs") {
        let Some(position) = faceoff.position else {
            continue;
        };
        let we_won = won.contains(&faceoff.at);
        let spot = nearest_dot(contested_point(position, we_won));
        add_spot_faceoffs(&mut spots, &[SpotFaceoffs { spot, won: u32::from(we_won), lost: u32::from(!we_won) }]);
    }
    spots
}

fn stat_rows(ours: &TeamSummary, theirs: &TeamSummary) -> Vec<TeamStatRow> {
    let row = |label: &str, a: &str, b: &str| TeamStatRow {
        group: "From the event export".to_owned(),
        label: label.to_owned(),
        ours: CellValue::from_text(a),
        theirs: CellValue::from_text(b),
    };
    let both = |label: &str, value: fn(&TeamSummary) -> u32| row(label, &value(ours).to_string(), &value(theirs).to_string());
    let power_plays = |s: &TeamSummary| format!("{} / {}", s.power_plays, s.power_play_goals);
    vec![
        both("Shot attempts", |s| s.shots),
        both("Shots on goal", |s| s.shots_on_goal),
        both("Scoring chances", |s| s.scoring_chances.0),
        both("Passes to the slot", |s| s.play.passes_to_slot),
        both("Shots blocked", |s| s.blocked_shots),
        both("Faceoffs won", |s| s.faceoffs_won),
        both("Puck battles won", |s| s.puck_battles_won),
        both("Zone entries", |s| s.play.entries.total()),
        both("Breakouts", |s| s.play.breakouts),
        both("Puck recoveries", |s| s.play.puck_recoveries),
        both("Puck losses", |s| s.play.puck_losses),
        both("Hits", |s| s.hits),
        both("Penalties", |s| s.penalties),
        row("Power plays / goals", &power_plays(ours), &power_plays(theirs)),
    ]
}

/// Skaters (by export name) and their guessed positions, goalies apart.
fn roster(side: &Side<'_>) -> (Vec<String>, HashMap<String, Position>) {
    let skaters: Vec<&str> = side.team.players().into_iter().filter(|n| !side.goalies.contains(*n)).collect();
    let positions = guess_positions(&side.team, &side.shifts, &skaters);
    (skaters.into_iter().map(str::to_owned).collect(), positions)
}

fn listed_skaters(side: &Side<'_>) -> Vec<OpponentSkater> {
    let (skaters, positions) = roster(side);
    skaters
        .iter()
        .map(|name| OpponentSkater {
            opponent: Opponent { jersey: None, surname: display_name(name) },
            position: positions.get(name).copied().unwrap_or(Position::Unknown),
            stats: skater_stats(name, side),
        })
        .collect()
}

/// A shot attempt with its spot, in the shooting team's frame.
struct PlacedShot {
    period: u32,
    shooter: Option<String>,
    at: RinkPoint,
    goal: bool,
}

fn placed_shots(side: &Side<'_>) -> Vec<PlacedShot> {
    let goals = side.team.moments("Goals");
    side.team
        .actions
        .iter()
        .filter(|a| a.name == "Shots")
        .filter_map(|a| Some(PlacedShot { period: a.period, shooter: a.player.clone(), at: point(a.position?), goal: goals.contains(&a.at) }))
        .collect()
}

fn opponent_shots(side: &Side<'_>) -> Vec<OpponentShot> {
    placed_shots(side).into_iter().map(|s| OpponentShot { period: s.period, jersey: None, at: s.at, goal: s.goal }).collect()
}

/// Our game's id: the date, the opponent and the score from our side.
fn game_id(title: &Title, ours: usize) -> GameId {
    let (goals_for, goals_against) = if ours == 0 { title.score } else { (title.score.1, title.score.0) };
    GameId(format!("{}_{}_{goals_for}-{goals_against}", title.date, title.teams[1 - ours].slug()))
}

fn league_game_id(title: &Title) -> GameId {
    GameId(format!("league_{}_{}_{}-{}_{}", title.date, title.teams[0].slug(), title.score.0, title.score.1, title.teams[1].slug()))
}

/// Both files of one game: the players file, and the team file if it was loaded.
pub struct EventGame<'a> {
    pub players: &'a EventFile,
    pub team: Option<&'a EventFile>,
}

impl EventGame<'_> {
    #[must_use]
    pub const fn title(&self) -> &Title {
        &self.players.title
    }

    fn check(&self) -> Result<(), Error> {
        if self.players.kind != EventFileKind::Players {
            return Err(Error::parse(SECTION, "a game needs its players file (the one with shifts)"));
        }
        if let Some(team) = self.team
            && (team.kind != EventFileKind::Team || team.title != self.players.title)
        {
            return Err(Error::parse(SECTION, "the team file is for a different game"));
        }
        Ok(())
    }

    fn warnings(&self, clock: &Clock) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.team.is_none() {
            warnings.push("the team file isn't loaded, so there are no scoring chances and power plays come from shifts".to_owned());
        }
        for period in clock.irregular_periods() {
            warnings.push(format!("period {period}'s shifts don't add up to 20:00; part of the video may be missing"));
        }
        warnings
    }
}

fn player(name: &str, id: PlayerId, position: Position) -> Player {
    Player {
        id,
        name: display_name(name),
        surname: surname(name),
        jersey: None,
        position,
        skater: None,
        goalie: None,
        shifts: Vec::new(),
        history: Vec::new(),
        details: Vec::new(),
    }
}

/// A warning when the goals in the export don't add up to the file name's score, given from
/// `side`'s point of view.
fn score_mismatch(side: &Side<'_>, (named_for, named_against): (u32, u32)) -> Option<String> {
    let listed = |scored: bool| u32::try_from(side.goals.iter().filter(|g| g.scored == scored).count()).unwrap_or(u32::MAX);
    let (listed_for, listed_against) = (listed(true), listed(false));
    ((listed_for, listed_against) != (named_for, named_against))
        .then(|| format!("the file name says {named_for}–{named_against} but the export has {listed_for}–{listed_against}"))
}

/// Our game, from our team's point of view.
pub fn build_game(game: &EventGame<'_>, team: &TeamPrefix) -> Result<Game, Error> {
    game.check()?;
    let title = game.title();
    let ours = our_index(title, team)?;
    let (our_name, their_name) = (&title.teams[ours], &title.teams[1 - ours]);
    let clock = Clock::new(&game.players.spans);
    let us = side(game, our_name, their_name, &clock);
    let them = side(game, their_name, our_name, &clock);
    let (skaters, positions) = roster(&us);
    let ids: HashMap<String, PlayerId> = us.team.players().into_iter().map(|n| (n.to_owned(), PlayerId::new(our_name, n))).collect();
    let id_of = |name: &str| ids.get(name).cloned().ok_or_else(|| Error::parse(SECTION, format!("no id for \"{name}\"")));
    let mut players = Vec::new();
    for name in &skaters {
        players.push(Player {
            skater: Some(skater_stats(name, &us)),
            shifts: us.shifts.get(name).cloned().unwrap_or_default(),
            ..player(name, id_of(name)?, positions.get(name).copied().unwrap_or(Position::Unknown))
        });
    }
    for name in &us.goalies {
        players.push(Player { goalie: Some(goalie_stats(name, &us, &clock)), ..player(name, id_of(name)?, Position::Goalie) });
    }
    let goals = us
        .goals
        .iter()
        .map(|g| Goal {
            time: g.time,
            scored_by: if g.scored { Team::Us } else { Team::Them },
            strength: g.strength,
            score_after: g.score_after,
            on_ice: g.on_ice.iter().filter_map(|n| ids.get(n).cloned()).collect(),
        })
        .collect();
    let passes = PlayerMatrix {
        values: skaters.iter().map(|a| skaters.iter().map(|b| us.passes.get(&(a.clone(), b.clone())).copied().unwrap_or(0)).collect()).collect(),
        players: skaters.iter().map(|n| id_of(n)).collect::<Result<_, _>>()?,
    };
    let (summary, opponent_summary) = (summary(&us, &clock), summary(&them, &clock));
    let (goals_for, goals_against) = if ours == 0 { title.score } else { (title.score.1, title.score.0) };
    let mut warnings = game.warnings(&clock);
    warnings.extend(score_mismatch(&us, (goals_for, goals_against)));
    Ok(Game {
        id: game_id(title, ours),
        date: title.date,
        team: our_name.clone(),
        opponent: their_name.clone(),
        goals_for,
        goals_against,
        units: units(&us, &Roster { ids: &ids, positions: &positions }, &clock),
        passes: Some(passes),
        goals,
        advantages: us.advantages.iter().map(|(i, mine)| Advantage { interval: *i, team: if *mine { Team::Us } else { Team::Them } }).collect(),
        team_stats: stat_rows(&summary, &opponent_summary),
        shot_zones_against: shot_zones(&them.team.actions),
        matchups: matchups(&us, &ids),
        charted_shots: placed_shots(&us)
            .into_iter()
            .map(|s| ChartedShot { period: s.period, shooter: s.shooter.and_then(|n| ids.get(&n).cloned()), at: s.at, goal: s.goal })
            .collect(),
        charted_shots_against: opponent_shots(&them),
        faceoff_spots: faceoff_spots(&us),
        goal_plays: plays::goal_plays(game, our_name, &clock, &ids),
        opponent_skaters: listed_skaters(&them),
        summary,
        opponent_summary,
        players,
        length: clock.length(),
        warnings,
    })
}

/// A game between two other teams.
pub fn build_league_game(game: &EventGame<'_>) -> Result<LeagueGame, Error> {
    game.check()?;
    let title = game.title();
    let clock = Clock::new(&game.players.spans);
    let scores = [title.score.0, title.score.1];
    let (first, second) = (side(game, &title.teams[0], &title.teams[1], &clock), side(game, &title.teams[1], &title.teams[0], &clock));
    let mut warnings = game.warnings(&clock);
    warnings.extend(score_mismatch(&first, title.score));
    let league_side = |i: usize, s: &Side<'_>| LeagueSide {
        team: title.teams[i].clone(),
        goals: scores[i],
        summary: summary(s, &clock),
        skaters: listed_skaters(s),
        shots: opponent_shots(s),
    };
    Ok(LeagueGame { id: league_game_id(title), date: title.date, sides: [league_side(0, &first), league_side(1, &second)], warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GoalOrigin, PlayKind};
    use crate::parse::events::parse;

    const OURS: [&str; 6] = ["McDavid Connor", "Draisaitl Leon", "Hyman Zach", "Bouchard Evan", "Ekholm Mattias", "Skinner Stuart"];
    const THEIRS: [&str; 6] = ["Crosby Sidney", "Malkin Evgeni", "Rust Bryan", "Letang Kris", "Karlsson Erik", "Jarry Tristan"];
    const NAME: &str = "Team One 1 _ 0 Team Two 26.09.2026.csv";

    /// One period with a whistle at 10:00 (video 600–630), then a second period.
    fn csv(extra: &[String]) -> String {
        let mut lines = vec!["ID,start,end,duration,pos_x,pos_y,player,team,action,half".to_owned()];
        for (team, names) in [("Team One", OURS), ("Team Two", THEIRS)] {
            for name in names {
                for (start, end, half) in [(0, 600, 1), (630, 1230, 1), (1300, 2500, 2)] {
                    lines.push(format!("0,{start},{end},{},,,{name},{team},All shifts,{half}", end - start));
                }
            }
        }
        lines.extend(extra.iter().cloned());
        lines.join("\n")
    }

    /// An action as InStat lists it: a 12-second clip centred on `moment`.
    fn action(moment: u32, at: Option<(f64, f64)>, player: &str, team: &str, name: &str) -> String {
        let (x, y) = at.map_or((String::new(), String::new()), |(x, y)| (x.to_string(), y.to_string()));
        format!("0,{},{},12,{x},{y},{player},{team},{name},1", moment - 6, moment + 6)
    }

    fn game(extra: &[String]) -> Game {
        let players = parse(csv(extra).as_bytes(), NAME).unwrap();
        let team = TeamPrefix::parse("Team One").unwrap();
        build_game(&EventGame { players: &players, team: None }, &team).unwrap()
    }

    fn skater<'a>(game: &'a Game, name: &str) -> &'a SkaterStats {
        game.players.iter().find(|p| p.name == name).and_then(|p| p.skater.as_ref()).unwrap()
    }

    fn goal_and_attempts() -> Vec<String> {
        let slot = Some((54.56, 12.96));
        let mut rows: Vec<String> = ["Shots", "Shots on goal", "Goals"].iter().map(|a| action(200, slot, OURS[0], "Team One", a)).collect();
        rows.extend(["Shots against", "Goals against"].iter().map(|a| action(200, None, THEIRS[5], "Team Two", a)));
        rows.push(action(300, slot, THEIRS[0], "Team Two", "Shots"));
        rows.extend(["Shots against", "Saves"].iter().map(|a| action(300, None, OURS[5], "Team One", a)));
        rows
    }

    #[test]
    fn a_goal_counts_for_everyone_on_the_ice() {
        let game = game(&goal_and_attempts());

        let mcdavid = skater(&game, "Connor McDavid");

        assert_eq!((mcdavid.goals, mcdavid.plus_minus), (1, 1));
        assert_eq!((mcdavid.corsi_for, mcdavid.corsi_against), (1, 1));
        assert_eq!(skater(&game, "Evan Bouchard").plus_minus, 1);
        assert_eq!(game.goals[0].time, Seconds(200.0));
        assert_eq!(game.goals[0].on_ice.len(), 5);
    }

    #[test]
    fn goalies_are_told_apart_by_the_shots_they_face() {
        let game = game(&goal_and_attempts());

        let goalie = game.players.iter().find(|p| p.name == "Stuart Skinner").unwrap();

        assert_eq!(goalie.position, Position::Goalie);
        assert_eq!(goalie.goalie.as_ref().map(|g| (g.shots_against, g.saves)), Some((1, 1)));
    }

    #[test]
    fn the_clock_stops_between_whistle_and_faceoff() {
        let clock = Clock::new(&parse(csv(&[]).as_bytes(), NAME).unwrap().spans);

        let after_whistle = clock.at(1, VideoTime::parse("700", "start").unwrap());

        assert_eq!(after_whistle, Seconds(670.0));
        assert_eq!(clock.at(2, VideoTime::parse("1300", "start").unwrap()), Seconds(1200.0));
    }

    #[test]
    fn a_period_missing_from_the_video_is_reported() {
        let game = game(&[]);

        assert_eq!(game.length, Seconds(2400.0));
        assert!(game.warnings.iter().any(|w| w.contains("period 3")), "{:?}", game.warnings);
    }

    #[test]
    fn a_shift_over_a_whistle_is_one_shift_but_periods_split_shifts() {
        let game = game(&[]);

        assert_eq!(skater(&game, "Connor McDavid").shifts, 2);
    }

    #[test]
    fn a_lost_faceoff_is_placed_from_our_end() {
        let at = Some((9.45, 19.67));
        let game = game(&[
            action(106, at, OURS[0], "Team One", "Faceoffs"),
            action(106, at, THEIRS[0], "Team Two", "Faceoffs"),
            action(106, at, THEIRS[0], "Team Two", "Faceoffs won"),
        ]);

        assert_eq!(game.faceoff_spots, vec![SpotFaceoffs { spot: FaceoffSpot::TheirZoneRight, won: 0, lost: 1 }]);
    }

    #[test]
    fn a_goal_soon_after_a_faceoff_comes_off_the_draw() {
        let at = Some((51.51, 6.25));
        let mut rows = vec![
            action(194, at, OURS[0], "Team One", "Faceoffs"),
            action(194, at, OURS[0], "Team One", "Faceoffs won"),
            action(194, at, THEIRS[0], "Team Two", "Faceoffs"),
            action(194, at, THEIRS[0], "Team Two", "Faceoffs lost"),
        ];
        rows.extend(goal_and_attempts());

        let play = &game(&rows).goal_plays[0];

        assert_eq!((play.scored_by, play.origin, play.start), (Team::Us, GoalOrigin::Faceoff, Seconds(194.0)));
        let faceoffs: Vec<&str> = play.events.iter().filter(|e| e.kind == PlayKind::Faceoff).map(|e| e.detail.as_str()).collect();
        assert_eq!(faceoffs, ["Faceoff won against Sidney Crosby"]);
    }

    #[test]
    fn a_goal_soon_after_entering_the_zone_comes_off_the_rush() {
        let mut rows = vec![action(193, Some((38.0, 12.0)), OURS[1], "Team One", "Entries"), action(193, Some((38.0, 12.0)), OURS[1], "Team One", "Entries via stickhandling")];
        rows.extend(goal_and_attempts());

        let play = &game(&rows).goal_plays[0];

        assert_eq!(play.origin, GoalOrigin::Rush);
        assert!(play.events.iter().any(|e| e.detail == "Carried in" && e.name.as_deref() == Some("Leon Draisaitl")));
    }

    #[test]
    fn their_actions_are_placed_from_our_end() {
        let rows = vec![
            action(150, Some((54.56, 12.96)), THEIRS[0], "Team Two", "Shots"),
            action(150, Some((54.56, 12.96)), THEIRS[0], "Team Two", "Shots on goal"),
            action(150, Some((54.56, 12.96)), THEIRS[0], "Team Two", "Goals"),
        ];

        let play = &game(&rows).goal_plays[0];

        assert_eq!(play.scored_by, Team::Them);
        let at = play.events[0].at.unwrap();
        assert!((at.along.0 + 79.0).abs() < 0.5, "{at:?}");
    }

    #[test]
    fn shot_zones_follow_distance_and_width() {
        let at = |out: f64, across: f64| RinkPoint { along: Feet(89.0 - out), across: Feet(across) };

        assert_eq!(shot_zone(at(10.0, 0.0)), ShotZone::Slot);
        assert_eq!(shot_zone(at(25.0, 5.0)), ShotZone::Center);
        assert_eq!(shot_zone(at(10.0, -25.0)), ShotZone::LeftFlank);
        assert_eq!(shot_zone(at(10.0, 25.0)), ShotZone::RightFlank);
        assert_eq!(shot_zone(at(50.0, 0.0)), ShotZone::BlueLineCenter);
        assert_eq!(shot_zone(at(50.0, -30.0)), ShotZone::BlueLineLeft);
    }

    #[test]
    fn a_missing_team_file_and_a_wrong_score_are_reported() {
        let game = game(&[]);

        assert!(game.warnings.iter().any(|w| w.contains("team file")), "{:?}", game.warnings);
        assert!(game.warnings.iter().any(|w| w.contains("says 1–0 but the export has 0–0")), "{:?}", game.warnings);
    }
}
