//! The run of play before each goal: every action from the last faceoff to the goal, and how
//! the goal came about.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{Clock, EventGame, contested_point, display_name, point, turned};
use crate::model::{GoalOrigin, GoalPlay, PlayEvent, PlayKind, PlayerId, RinkPoint, Seconds, Team, TeamName};
use crate::parse::events::VideoTime;

/// A goal this soon after its faceoff came off the draw.
const OFF_THE_FACEOFF: f64 = 10.0;
/// A goal this soon after a zone entry, or after winning the puck back in the zone, came
/// from it.
const QUICK_STRIKE: f64 = 10.0;
/// A shot on goal by the scoring team this soon before the goal makes it a rebound.
const REBOUND: f64 = 3.0;

/// Everything one player of one team did at one moment (or the team, for unnamed rows).
struct Moment<'a> {
    team: Team,
    period: u32,
    at: VideoTime,
    player: Option<&'a str>,
    names: BTreeSet<&'a str>,
    position: Option<(f64, f64)>,
}

impl Moment<'_> {
    fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    fn is_contested(&self) -> bool {
        self.has("Faceoffs") || self.has("Puck battles")
    }

    fn won(&self) -> bool {
        self.has("Faceoffs won") || self.has("Puck battles won")
    }

    /// Where it happened, seen from our end.
    fn spot(&self) -> Option<RinkPoint> {
        let position = self.position?;
        Some(if self.is_contested() {
            contested_point(position, self.won() == (self.team == Team::Us))
        } else if self.team == Team::Us {
            point(position)
        } else {
            point(turned(position))
        })
    }

    fn outcome(&self, won: &str, lost: &str) -> String {
        (if self.won() { won } else { lost }).to_owned()
    }

    /// The most telling thing that happened, in words; `None` for rows that only repeat
    /// another (a goalie's save of a shot already listed, an assist named on the goal).
    fn kind_and_detail(&self) -> Option<(PlayKind, String)> {
        let pick = |options: &[(&str, &str)], otherwise: &str| {
            options.iter().find(|(name, _)| self.has(name)).map_or(otherwise, |(_, text)| text).to_owned()
        };
        Some(if self.has("Goals") {
            (PlayKind::Goal, "Goal".to_owned())
        } else if self.has("Shots") {
            let result = [("Shots on goal", "Shot on goal"), ("Blocked shots", "Shot blocked"), ("Missed shots", "Shot missed")];
            (PlayKind::Shot, pick(&result, "Shot"))
        } else if self.has("Faceoffs") {
            (PlayKind::Faceoff, self.outcome("Faceoff won", "Faceoff lost"))
        } else if self.has("Puck battles") {
            (PlayKind::Battle, self.outcome("Puck battle won", "Puck battle lost"))
        } else if self.has("Entries") {
            let how = [("Entries via stickhandling", "Carried in"), ("Entries via pass", "Passed in"), ("Entries via dump in", "Dumped in")];
            (PlayKind::Entry, pick(&how, "Zone entry"))
        } else if self.has("Breakouts") {
            let how = [("Breakouts via stickhandling", "Carried out"), ("Breakouts via pass", "Passed out"), ("Breakouts via dump out", "Dumped out")];
            (PlayKind::Breakout, pick(&how, "Breakout"))
        } else if self.has("Dump ins") {
            (PlayKind::DumpIn, "Dump-in".to_owned())
        } else if self.has("Dump outs") {
            (PlayKind::DumpOut, "Dump-out".to_owned())
        } else if self.has("Accurate passes") {
            (PlayKind::Pass, pick(&[("Passes to the slot", "Pass into the slot")], "Pass"))
        } else if self.has("Inaccurate passes") {
            (PlayKind::PassMissed, "Pass missed".to_owned())
        } else if self.has("Puck recoveries") {
            (PlayKind::Recovery, "Puck recovered".to_owned())
        } else if self.has("Puck losses") {
            (PlayKind::Loss, "Puck lost".to_owned())
        } else if self.has("Shots blocking") {
            (PlayKind::Block, "Blocked a shot".to_owned())
        } else if self.has("Hits") {
            (PlayKind::Hit, "Hit".to_owned())
        } else if self.has("Penalties") {
            (PlayKind::Penalty, "Penalty".to_owned())
        } else {
            return None;
        })
    }
}

/// Each team's actions grouped by moment and player, in game order.
fn moments<'a>(game: &EventGame<'a>, ours: &TeamName) -> Vec<Moment<'a>> {
    let mut grouped: BTreeMap<(u32, VideoTime, bool, Option<&str>), Moment<'a>> = BTreeMap::new();
    for action in &game.players.actions {
        let team = if &action.team == ours { Team::Us } else { Team::Them };
        let player = action.player.as_deref();
        let moment = grouped.entry((action.period, action.at, team == Team::Us, player)).or_insert_with(|| Moment {
            team,
            period: action.period,
            at: action.at,
            player,
            names: BTreeSet::new(),
            position: None,
        });
        moment.names.insert(&action.name);
        moment.position = moment.position.or(action.position);
    }
    grouped.into_values().collect()
}

/// The scoring team's side of how the goal came about, from the plays before it.
fn origin(goal: &Moment<'_>, play: &[(Seconds, &Moment<'_>)], goal_time: Seconds, from_faceoff: Option<Seconds>) -> GoalOrigin {
    if from_faceoff.is_some_and(|start| goal_time.0 - start.0 <= OFF_THE_FACEOFF) {
        return GoalOrigin::Faceoff;
    }
    let recently = |name: &str| play.iter().any(|(t, m)| m.team == goal.team && goal_time.0 - t.0 <= QUICK_STRIKE && m.has(name));
    if recently("Puck recoveries in OZ") {
        GoalOrigin::Turnover
    } else if recently("Entries") {
        GoalOrigin::Rush
    } else {
        GoalOrigin::Sustained
    }
}

fn is_rebound(goal: &Moment<'_>, play: &[(Seconds, &Moment<'_>)], goal_time: Seconds) -> bool {
    play.iter().any(|(t, m)| m.team == goal.team && m.at < goal.at && goal_time.0 - t.0 <= REBOUND && m.has("Shots on goal"))
}

/// The other team's side of a faceoff or puck battle, logged at the same moment.
fn other_side<'a, 'b>(contest: &Moment<'_>, play: &'b [(Seconds, &'b Moment<'a>)]) -> Option<&'b Moment<'a>> {
    play.iter().map(|(_, m)| *m).find(|m| m.team != contest.team && m.at == contest.at && m.is_contested())
}

/// The action as one event: a contest only from its winner's side, naming who they beat.
fn describe(moment: &Moment<'_>, play: &[(Seconds, &Moment<'_>)], assists: &[String]) -> Option<(PlayKind, String)> {
    let (kind, detail) = moment.kind_and_detail()?;
    if kind == PlayKind::Goal && !assists.is_empty() {
        return Some((kind, format!("Goal, assisted by {}", assists.join(" and "))));
    }
    if !moment.is_contested() {
        return Some((kind, detail));
    }
    match other_side(moment, play) {
        Some(_) if !moment.won() => None,
        Some(loser) => Some((kind, loser.player.map_or_else(|| detail.clone(), |name| format!("{detail} against {}", display_name(name))))),
        None => Some((kind, detail)),
    }
}

fn assisted_by(goal: &Moment<'_>, play: &[(Seconds, &Moment<'_>)]) -> Vec<String> {
    play.iter()
        .filter(|(_, m)| m.team == goal.team && m.at == goal.at && m.has("Assists"))
        .filter_map(|(_, m)| m.player.map(display_name))
        .collect()
}

/// Every goal's run of play from its last faceoff (or the start of the period).
pub(super) fn goal_plays(game: &EventGame<'_>, ours: &TeamName, clock: &Clock, ids: &HashMap<String, PlayerId>) -> Vec<GoalPlay> {
    let moments = moments(game, ours);
    let faceoffs: Vec<(u32, VideoTime)> = moments.iter().filter(|m| m.has("Faceoffs")).map(|m| (m.period, m.at)).collect();
    moments
        .iter()
        .filter(|m| m.has("Goals"))
        .map(|goal| {
            let faceoff = faceoffs.iter().filter(|(p, t)| *p == goal.period && *t <= goal.at).map(|(_, t)| *t).max();
            let play: Vec<(Seconds, &Moment<'_>)> = moments
                .iter()
                .filter(|m| m.period == goal.period && faceoff.is_none_or(|f| m.at >= f) && m.at <= goal.at)
                .map(|m| (clock.at(m.period, m.at), m))
                .collect();
            let time = clock.at(goal.period, goal.at);
            let from_faceoff = faceoff.map(|f| clock.at(goal.period, f));
            let assists = assisted_by(goal, &play);
            let events = play
                .iter()
                .filter_map(|(t, m)| {
                    let (kind, detail) = describe(m, &play, &assists)?;
                    Some(PlayEvent {
                        time: *t,
                        team: m.team,
                        player: m.player.filter(|_| m.team == Team::Us).and_then(|p| ids.get(p)).cloned(),
                        name: m.player.map(display_name),
                        kind,
                        detail,
                        at: m.spot(),
                    })
                })
                .collect();
            GoalPlay {
                time,
                scored_by: goal.team,
                origin: origin(goal, &play, time, from_faceoff),
                rebound: is_rebound(goal, &play, time),
                start: from_faceoff.unwrap_or_else(|| clock.at(goal.period, VideoTime::START)),
                events,
            }
        })
        .collect()
}
