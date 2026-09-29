//! Hockey analysis over a chosen set of games: builds the full report the UI renders.

pub mod common;
pub mod goalies;
pub mod impact;
pub mod models;
pub mod pairs;
pub mod passing;
pub mod players;
pub mod profiles;
pub mod significance;
pub mod stints;
pub mod team;
pub mod units;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Date, Game, GameId, PlayerId, Seconds, Strength, Team, TeamStatRow, UnitKind, UnitStats};
use common::{PlayerRef, TestRow, adjust_families};
use impact::Ratings;
use pairs::PairRow;

/// What the coach asked to see.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Games in scope; empty means every loaded game.
    pub games: Vec<GameId>,
    /// A single game to compare against the rest of the scope.
    pub focus: Option<GameId>,
    /// Players and units below this many minutes are shown but not ranked.
    pub min_minutes: f64,
    pub min_unit_minutes: f64,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            focus: None,
            min_minutes: 10.0,
            min_unit_minutes: 3.0,
        }
    }
}

/// Everything the analysis modules share.
pub struct Context<'a> {
    pub scope: Vec<&'a Game>,
    pub focus: Option<&'a Game>,
    pub roster: HashMap<PlayerId, PlayerRef>,
    pub stints: Vec<stints::Stint>,
    pub min_toi: Seconds,
    pub min_unit_toi: Seconds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameListing {
    pub id: GameId,
    pub date: Date,
    pub opponent: String,
    pub goals_for: u32,
    pub goals_against: u32,
    pub outcome: team::Outcome,
    pub in_scope: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimelinePlayer {
    pub player: PlayerRef,
    pub group: Option<String>,
    pub shifts: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimelineGoal {
    pub time: f64,
    pub scored_by: Team,
    pub strength: Strength,
    pub score: (u32, u32),
    pub on_ice: Vec<PlayerId>,
}

/// Shift chart data for one game.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameTimeline {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub length: f64,
    pub players: Vec<TimelinePlayer>,
    pub goals: Vec<TimelineGoal>,
    /// (start, end, team with the advantage).
    pub advantages: Vec<(f64, f64, Team)>,
    pub goals_for: u32,
    pub goals_against: u32,
    pub team_stats: Vec<TeamStatRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Analysis {
    pub team_name: String,
    pub games: Vec<GameListing>,
    pub focus: Option<GameId>,
    pub request: Request,
    pub team: team::TeamReport,
    pub players: Vec<players::PlayerSeason>,
    pub goalies: Vec<goalies::GoalieSeason>,
    pub units: units::UnitsReport,
    pub pairs: Vec<PairRow>,
    pub passing: passing::PassingReport,
    pub impact: impact::ImpactReport,
    pub tests: Vec<TestRow>,
    pub power: Vec<significance::PowerRow>,
    pub models: models::ModelsReport,
    pub profiles: profiles::ProfilesReport,
    pub timelines: Vec<GameTimeline>,
}

fn timeline(game: &Game, roster: &HashMap<PlayerId, PlayerRef>) -> GameTimeline {
    GameTimeline {
        game: game.id.clone(),
        date: game.date,
        opponent: game.opponent.0.clone(),
        length: game.length.0,
        players: game
            .players
            .iter()
            .filter(|p| !p.shifts.is_empty())
            .filter_map(|p| {
                Some(TimelinePlayer {
                    player: roster.get(&p.id)?.clone(),
                    group: p.group.clone(),
                    shifts: p.shifts.iter().map(|s| (s.start.0, s.end.0)).collect(),
                })
            })
            .collect(),
        goals: game
            .goals
            .iter()
            .map(|g| TimelineGoal {
                time: g.time.0,
                scored_by: g.scored_by,
                strength: g.strength,
                score: g.score_after,
                on_ice: g.on_ice.clone(),
            })
            .collect(),
        advantages: game
            .advantages
            .iter()
            .map(|a| (a.interval.start.0, a.interval.end.0, a.team))
            .collect(),
        goals_for: game.goals_for,
        goals_against: game.goals_against,
        team_stats: game.team_stats.clone(),
    }
}

/// Attempt-weighted expected CF% over the line-table units in which both players appear.
fn fill_pair_expectations(context: &Context<'_>, pairs: &mut [PairRow], ratings: &HashMap<UnitKind, Ratings>) {
    for pair in pairs {
        let Some(corsi) = pair.corsi.as_mut() else {
            continue;
        };
        let Some(model) = ratings.get(&corsi.source) else {
            continue;
        };
        let (mut weighted, mut weight) = (0.0, 0.0);
        for unit in context
            .scope
            .iter()
            .flat_map(|g| g.units.iter())
            .filter(|u| u.kind == corsi.source && u.players.contains(&pair.a.id) && u.players.contains(&pair.b.id))
        {
            let UnitStats::EvenStrength(stats) = &unit.stats else {
                continue;
            };
            let attempts = f64::from(stats.corsi_for + stats.corsi_against);
            let members: Vec<&PlayerId> = unit.players.iter().collect();
            if let Some(expected) = model.expected_share(&members) {
                weighted += attempts * expected;
                weight += attempts;
            }
        }
        corsi.expected_pct = (weight > 0.0).then(|| weighted / weight);
    }
}

#[must_use]
pub fn analyse(all: &[Game], request: &Request) -> Analysis {
    let mut sorted: Vec<&Game> = all.iter().collect();
    sorted.sort_by_key(|g| g.date);
    let scope: Vec<&Game> = sorted
        .iter()
        .copied()
        .filter(|g| request.games.is_empty() || request.games.contains(&g.id))
        .collect();
    let focus = request
        .focus
        .as_ref()
        .and_then(|id| scope.iter().copied().find(|g| &g.id == id));
    let roster = common::roster(&sorted);
    let context = Context {
        stints: scope.iter().flat_map(|g| stints::stints(g)).collect(),
        scope,
        focus,
        roster,
        min_toi: Seconds(request.min_minutes * 60.0),
        min_unit_toi: Seconds(request.min_unit_minutes * 60.0),
    };

    let team_report = team::team(&context);
    let corsi_prior = players::corsi_prior(&context);
    let player_seasons = players::player_seasons(&context, corsi_prior);
    let units_report = units::units(&context);
    let (passing_report, pass_totals) = passing::passing(&context);
    let impact_outputs = impact::impact(&context);
    let mut pair_rows = pairs::pairs(&context, &pass_totals, &units_report.priors);
    let ratings: HashMap<UnitKind, Ratings> = [
        (UnitKind::DefencePair, impact_outputs.defence_ratings),
        (UnitKind::ForwardLine, impact_outputs.forward_ratings),
        (UnitKind::FullUnit, impact_outputs.full_unit_ratings),
    ]
    .into_iter()
    .filter_map(|(k, r)| Some((k, r?)))
    .collect();
    fill_pair_expectations(&context, &mut pair_rows, &ratings);

    let significance::Significance { mut tests, power } =
        significance::significance(&context, &units_report, &pair_rows, &passing_report, &team_report);
    let models_report = models::models(&context, &mut tests);
    let profiles_report = profiles::profiles(&player_seasons, &mut tests);
    adjust_families(&mut tests);

    let focus_id = context.focus.map(|g| g.id.clone());
    Analysis {
        team_name: context.scope.first().or_else(|| sorted.first()).map(|g| g.team.0.clone()).unwrap_or_default(),
        games: sorted
            .iter()
            .map(|g| GameListing {
                id: g.id.clone(),
                date: g.date,
                opponent: g.opponent.0.clone(),
                goals_for: g.goals_for,
                goals_against: g.goals_against,
                outcome: team::outcome(g),
                in_scope: context.scope.iter().any(|s| s.id == g.id),
                warnings: g.warnings.clone(),
            })
            .collect(),
        focus: focus_id,
        request: request.clone(),
        team: team_report,
        players: player_seasons,
        goalies: goalies::goalies(&context),
        units: units_report,
        pairs: pair_rows,
        passing: passing_report,
        impact: impact_outputs.report,
        tests,
        power,
        models: models_report,
        profiles: profiles_report,
        timelines: context.scope.iter().map(|g| timeline(g, &context.roster)).collect(),
    }
}
