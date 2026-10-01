//! Our team and players against the rest of the league: every other team's games (league
//! match reports plus our opponents' side of our own games) set the averages; our own team
//! never counts toward them.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::PlayerRef;
use crate::analysis::players::{PlayerSeason, SkaterTotals};
use crate::analysis::rankings::{self, ListedAppearance, RatingInput, Side, league_appearances, listed_inputs, listed_key, opponent_appearances};
use crate::analysis::rating_setup::{RatingStat, RatingWeights};
use crate::analysis::team::{Outcome, outcome};
use crate::model::{Date, Game, GameId, PERIOD_SECONDS, PlayerId, Position, TeamName, TeamSummary};
use crate::stats::describe::{mean, sample_sd};

/// Ice time that makes a single game count in the per-game rating pool.
const MIN_GAME_SECONDS: f64 = 300.0;

/// One team's numbers in one game.
#[derive(Debug, Clone, Copy)]
struct TeamGame<'a> {
    team: &'a TeamName,
    ours: bool,
    own: &'a TeamSummary,
    other: &'a TeamSummary,
    goals_for: u32,
    goals_against: u32,
    outcome: Outcome,
}

#[derive(Debug, Clone, Copy)]
enum Measure {
    /// Averaged over games.
    PerGame(fn(&TeamGame<'_>) -> Option<f64>),
    /// (part, whole) pooled over games, as a percentage.
    Rate(fn(&TeamGame<'_>) -> (f64, f64)),
}

struct TeamStat {
    name: &'static str,
    measure: Measure,
    higher_is_better: bool,
}

fn n(v: u32) -> f64 {
    f64::from(v)
}

const TEAM_STATS: [TeamStat; 22] = [
    TeamStat { name: "Goals for", measure: Measure::PerGame(|t| Some(n(t.goals_for))), higher_is_better: true },
    TeamStat { name: "Goals against", measure: Measure::PerGame(|t| Some(n(t.goals_against))), higher_is_better: false },
    TeamStat { name: "Shot attempts for", measure: Measure::PerGame(|t| Some(n(t.own.shots))), higher_is_better: true },
    TeamStat { name: "Shot attempts against", measure: Measure::PerGame(|t| Some(n(t.other.shots))), higher_is_better: false },
    TeamStat { name: "Shot attempt share", measure: Measure::Rate(|t| (n(t.own.shots), n(t.own.shots + t.other.shots))), higher_is_better: true },
    TeamStat { name: "Shots on goal for", measure: Measure::PerGame(|t| Some(n(t.own.shots_on_goal))), higher_is_better: true },
    TeamStat { name: "Shots on goal against", measure: Measure::PerGame(|t| Some(n(t.other.shots_on_goal))), higher_is_better: false },
    TeamStat { name: "xG for", measure: Measure::PerGame(|t| t.own.xg), higher_is_better: true },
    TeamStat { name: "xG against", measure: Measure::PerGame(|t| t.other.xg), higher_is_better: false },
    TeamStat {
        name: "Expected-goals share",
        measure: Measure::Rate(|t| match (t.own.xg, t.other.xg) {
            (Some(a), Some(b)) => (a, a + b),
            _ => (0.0, 0.0),
        }),
        higher_is_better: true,
    },
    TeamStat { name: "Scoring chances for", measure: Measure::PerGame(|t| Some(n(t.own.scoring_chances.0))), higher_is_better: true },
    TeamStat { name: "Scoring chances against", measure: Measure::PerGame(|t| Some(n(t.other.scoring_chances.0))), higher_is_better: false },
    TeamStat { name: "Shooting %", measure: Measure::Rate(|t| (n(t.goals_for), n(t.own.shots_on_goal))), higher_is_better: true },
    TeamStat {
        name: "Save %",
        measure: Measure::Rate(|t| (n(t.other.shots_on_goal.saturating_sub(t.goals_against)), n(t.other.shots_on_goal))),
        higher_is_better: true,
    },
    TeamStat { name: "Faceoff win %", measure: Measure::Rate(|t| (n(t.own.faceoffs_won), n(t.own.faceoffs_won + t.other.faceoffs_won))), higher_is_better: true },
    TeamStat {
        name: "Puck battles won %",
        measure: Measure::Rate(|t| (n(t.own.puck_battles_won), n(t.own.puck_battles_won + t.other.puck_battles_won))),
        higher_is_better: true,
    },
    TeamStat { name: "Possession %", measure: Measure::PerGame(|t| t.own.possession_pct), higher_is_better: true },
    TeamStat { name: "Power play %", measure: Measure::Rate(|t| (n(t.own.power_play_goals), n(t.own.power_plays))), higher_is_better: true },
    TeamStat {
        name: "Penalty kill %",
        measure: Measure::Rate(|t| (n(t.other.power_plays.saturating_sub(t.other.power_play_goals)), n(t.other.power_plays))),
        higher_is_better: true,
    },
    TeamStat { name: "Times short-handed", measure: Measure::PerGame(|t| Some(n(t.other.power_plays))), higher_is_better: false },
    TeamStat { name: "Hits", measure: Measure::PerGame(|t| Some(n(t.own.hits))), higher_is_better: true },
    TeamStat { name: "Blocked shots", measure: Measure::PerGame(|t| Some(n(t.own.blocked_shots))), higher_is_better: true },
];

impl Measure {
    /// The stat over a set of games: the per-game mean, or the pooled rate.
    fn over(self, games: &[&TeamGame<'_>]) -> Option<f64> {
        match self {
            Self::PerGame(f) => mean(&games.iter().filter_map(|g| f(g)).collect::<Vec<_>>()),
            Self::Rate(f) => {
                let (part, whole) = games.iter().map(|g| f(g)).fold((0.0, 0.0), |(p, w), (a, b)| (p + a, w + b));
                (whole > 0.0).then(|| 100.0 * part / whole)
            }
        }
    }

    /// The stat in one game.
    fn single(self, game: &TeamGame<'_>) -> Option<f64> {
        self.over(&[game])
    }
}

/// Goals in the three regulation periods, when the report breaks them out.
fn regulation_goals(summary: &TeamSummary) -> Option<u32> {
    (summary.shots_by_period.len() >= 3).then(|| summary.shots_by_period.iter().take(3).map(|p| p.2).sum())
}

/// A league side's result: level after regulation and then beaten means an overtime loss.
fn league_outcome(goals_for: u32, goals_against: u32, own: &TeamSummary, other: &TeamSummary) -> Outcome {
    match goals_for.cmp(&goals_against) {
        std::cmp::Ordering::Greater => Outcome::Win,
        std::cmp::Ordering::Equal => Outcome::Tie,
        std::cmp::Ordering::Less => match (regulation_goals(own), regulation_goals(other)) {
            (Some(a), Some(b)) if a == b => Outcome::OvertimeLoss,
            _ => Outcome::Loss,
        },
    }
}

/// The opponent's result in one of our games.
fn mirrored(game: &Game) -> Outcome {
    let overtime = game.length.0 > 3.0 * PERIOD_SECONDS + 1.0;
    match outcome(game) {
        Outcome::Win if overtime => Outcome::OvertimeLoss,
        Outcome::Win => Outcome::Loss,
        Outcome::Loss | Outcome::OvertimeLoss => Outcome::Win,
        Outcome::Tie => Outcome::Tie,
    }
}

fn our_side(game: &Game) -> TeamGame<'_> {
    TeamGame {
        team: &game.team,
        ours: true,
        own: &game.summary,
        other: &game.opponent_summary,
        goals_for: game.goals_for,
        goals_against: game.goals_against,
        outcome: outcome(game),
    }
}

fn team_games<'a>(context: &Context<'a>) -> Vec<TeamGame<'a>> {
    let ours = context.scope.iter().map(|g| our_side(g));
    let opponents = context.all_games.iter().map(|g| TeamGame {
        team: &g.opponent,
        ours: false,
        own: &g.opponent_summary,
        other: &g.summary,
        goals_for: g.goals_against,
        goals_against: g.goals_for,
        outcome: mirrored(g),
    });
    let league = context.league.iter().flat_map(|g| {
        (0..2).map(move |i| {
            let (own, other) = (&g.sides[i], &g.sides[1 - i]);
            TeamGame {
                team: &own.team,
                ours: false,
                own: &own.summary,
                other: &other.summary,
                goals_for: own.goals,
                goals_against: other.goals,
                outcome: league_outcome(own.goals, other.goals, &own.summary, &other.summary),
            }
        })
    });
    ours.chain(opponents).chain(league).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TeamStatRow {
    pub stat: String,
    pub higher_is_better: bool,
    pub ours: Option<f64>,
    /// Every other team's games together.
    pub league: Option<f64>,
    /// Our place among all teams, 1 = best.
    pub rank: Option<usize>,
    pub teams_ranked: usize,
    /// Each team's value, so any one team can be compared with us.
    pub by_team: Vec<(String, Option<f64>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GameStatValue {
    pub value: Option<f64>,
    /// Standard deviations from the league's typical team-game, sign as is.
    pub z: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameVsLeague {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub goals_for: u32,
    pub goals_against: u32,
    pub outcome: Outcome,
    /// Indexed like `stats`.
    pub values: Vec<GameStatValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TeamStanding {
    pub team: String,
    pub ours: bool,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub overtime_losses: u32,
    pub ties: u32,
    pub points: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub attempt_share: Option<f64>,
    pub xg_share: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerStatVsLeague {
    pub stat: RatingStat,
    pub value: Option<f64>,
    /// Average of league skaters at the same position.
    pub league: Option<f64>,
    /// Share of league skaters at the position this player is better than (50 = average).
    pub percentile: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameRatingVsLeague {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    /// 50 = an average league skater's game at the position.
    pub rating: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerVsLeague {
    pub player: PlayerRef,
    pub games: u32,
    pub toi: f64,
    pub qualified: bool,
    pub stats: Vec<PlayerStatVsLeague>,
    pub average_percentile: Option<f64>,
    pub game_ratings: Vec<GameRatingVsLeague>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatInfo {
    pub stat: RatingStat,
    pub name: String,
    pub higher_is_better: bool,
}

/// A loaded league game, for the games list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeagueListing {
    pub id: GameId,
    pub date: Date,
    pub teams: [String; 2],
    pub score: (u32, u32),
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeagueReport {
    pub team_name: String,
    pub loaded: Vec<LeagueListing>,
    /// League match reports loaded (games between two other teams).
    pub league_games: usize,
    /// Other teams' games behind the averages, including our opponents' side of our games.
    pub reference_games: usize,
    pub teams: Vec<String>,
    pub reference_forwards: usize,
    pub reference_defence: usize,
    pub stats: Vec<TeamStatRow>,
    pub games: Vec<GameVsLeague>,
    pub standings: Vec<TeamStanding>,
    pub player_stats: Vec<StatInfo>,
    pub players: Vec<PlayerVsLeague>,
}

fn rank(ours: f64, others: &[f64], higher_is_better: bool) -> usize {
    1 + others.iter().filter(|&&v| if higher_is_better { v > ours } else { v < ours }).count()
}

fn team_stats(games: &[TeamGame<'_>]) -> Vec<TeamStatRow> {
    let reference: Vec<&TeamGame<'_>> = games.iter().filter(|g| !g.ours).collect();
    let ours: Vec<&TeamGame<'_>> = games.iter().filter(|g| g.ours).collect();
    let mut by_team: BTreeMap<&TeamName, Vec<&TeamGame<'_>>> = BTreeMap::new();
    for g in &reference {
        by_team.entry(g.team).or_default().push(g);
    }
    TEAM_STATS
        .iter()
        .map(|stat| {
            let our_value = stat.measure.over(&ours);
            let team_values: Vec<(String, Option<f64>)> = by_team.iter().map(|(team, gs)| (team.0.clone(), stat.measure.over(gs))).collect();
            let others: Vec<f64> = team_values.iter().filter_map(|(_, v)| *v).collect();
            TeamStatRow {
                stat: stat.name.to_owned(),
                higher_is_better: stat.higher_is_better,
                league: stat.measure.over(&reference),
                rank: our_value.map(|v| rank(v, &others, stat.higher_is_better)),
                teams_ranked: others.len() + usize::from(our_value.is_some()),
                ours: our_value,
                by_team: team_values,
            }
        })
        .collect()
}

fn games_vs_league(context: &Context<'_>, games: &[TeamGame<'_>]) -> Vec<GameVsLeague> {
    let reference: Vec<&TeamGame<'_>> = games.iter().filter(|g| !g.ours).collect();
    let spread: Vec<Option<(f64, f64)>> = TEAM_STATS
        .iter()
        .map(|stat| {
            let values: Vec<f64> = reference.iter().filter_map(|g| stat.measure.single(g)).collect();
            mean(&values).zip(sample_sd(&values)).filter(|(_, sd)| *sd > 0.0)
        })
        .collect();
    context
        .scope
        .iter()
        .map(|game| (game, our_side(game)))
        .map(|(game, side)| GameVsLeague {
            game: game.id.clone(),
            date: game.date,
            opponent: game.opponent.0.clone(),
            goals_for: game.goals_for,
            goals_against: game.goals_against,
            outcome: side.outcome,
            values: TEAM_STATS
                .iter()
                .zip(&spread)
                .map(|(stat, s)| {
                    let value = stat.measure.single(&side);
                    GameStatValue { value, z: value.zip(*s).map(|(v, (m, sd))| (v - m) / sd) }
                })
                .collect(),
        })
        .collect()
}

fn standings(games: &[TeamGame<'_>]) -> Vec<TeamStanding> {
    let mut by_team: BTreeMap<(bool, &TeamName), Vec<&TeamGame<'_>>> = BTreeMap::new();
    for g in games {
        by_team.entry((g.ours, g.team)).or_default().push(g);
    }
    let mut rows: Vec<TeamStanding> = by_team
        .into_iter()
        .map(|((ours, team), gs)| {
            let count = |o: Outcome| u32::try_from(gs.iter().filter(|g| g.outcome == o).count()).unwrap_or(u32::MAX);
            let (wins, losses, overtime_losses, ties) = (count(Outcome::Win), count(Outcome::Loss), count(Outcome::OvertimeLoss), count(Outcome::Tie));
            TeamStanding {
                team: team.0.clone(),
                ours,
                games: u32::try_from(gs.len()).unwrap_or(u32::MAX),
                points: gs.iter().map(|g| g.outcome.points()).sum(),
                wins,
                losses,
                overtime_losses,
                ties,
                goals_for: gs.iter().map(|g| g.goals_for).sum(),
                goals_against: gs.iter().map(|g| g.goals_against).sum(),
                attempt_share: TEAM_STATS[4].measure.over(&gs),
                xg_share: TEAM_STATS[9].measure.over(&gs),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        let per_game = |r: &TeamStanding| f64::from(r.points) / f64::from(r.games.max(1));
        per_game(b).total_cmp(&per_game(a)).then(b.goals_for.cmp(&a.goals_for)).then(a.team.cmp(&b.team))
    });
    rows
}

/// Share of `others` this value beats, counting ties as half (50 = average).
fn percentile(value: f64, others: &[f64], higher_is_better: bool) -> Option<f64> {
    if others.is_empty() {
        return None;
    }
    let beaten = others.iter().filter(|&&o| if higher_is_better { value > o } else { value < o }).count() as f64;
    let tied = others.iter().filter(|&&o| (o - value).abs() < 1e-12).count() as f64;
    Some(100.0 * (beaten + tied / 2.0) / others.len() as f64)
}

fn compared_stats() -> Vec<RatingStat> {
    RatingStat::ALL.into_iter().filter(|s| !s.needs_players_report()).collect()
}

fn single_game_input(player: PlayerRef, side: Side, stats: &crate::model::SkaterStats, own: &TeamSummary, other: &TeamSummary) -> RatingInput {
    let mut totals = SkaterTotals::default();
    totals.add(stats);
    RatingInput {
        player,
        side,
        qualified: stats.toi.0 >= MIN_GAME_SECONDS,
        totals,
        team_even_strength: (n(own.even_strength_shots.0), n(other.even_strength_shots.0)),
    }
}

/// Our players' single-game ratings graded against every league skater's games at the position.
fn game_ratings(context: &Context<'_>, appearances: &[ListedAppearance<'_>], weights: &RatingWeights) -> BTreeMap<PlayerId, Vec<GameRatingVsLeague>> {
    let mut inputs: Vec<RatingInput> = appearances
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let id = format!("{}|{i}", listed_key(a.team, a.skater));
            let player = PlayerRef { id: PlayerId(id), name: a.skater.opponent.surname.clone(), jersey: None, position: a.skater.position };
            single_game_input(player, Side::Opponent, &a.skater.stats, a.own, a.other)
        })
        .collect();
    let mut games_of: BTreeMap<String, (PlayerId, &Game)> = BTreeMap::new();
    for game in &context.scope {
        for p in &game.players {
            let (Some(stats), Some(player)) = (p.skater.as_ref(), context.roster.get(&p.id)) else {
                continue;
            };
            let key = format!("{}|{}", p.id.0, game.id.0);
            games_of.insert(key.clone(), (p.id.clone(), game));
            let player = PlayerRef { id: PlayerId(key), ..player.clone() };
            inputs.push(single_game_input(player, Side::Ours, stats, &game.summary, &game.opponent_summary));
        }
    }
    let mut result: BTreeMap<PlayerId, Vec<GameRatingVsLeague>> = BTreeMap::new();
    for (position, position_weights) in [(Position::Forward, &weights.forwards), (Position::Defence, &weights.defence)] {
        for row in rankings::rate(&inputs, position, position_weights) {
            if let Some((player, game)) = games_of.get(&row.player.id.0) {
                result.entry(player.clone()).or_default().push(GameRatingVsLeague {
                    game: game.id.clone(),
                    date: game.date,
                    opponent: game.opponent.0.clone(),
                    rating: row.rating,
                });
            }
        }
    }
    for games in result.values_mut() {
        games.sort_by_key(|g| g.date);
    }
    result
}

fn players_vs_league(context: &Context<'_>, seasons: &[PlayerSeason], weights: &RatingWeights) -> (Vec<PlayerVsLeague>, usize, usize) {
    let appearances = [opponent_appearances(&context.all_games), league_appearances(&context.league)].concat();
    let reference: Vec<RatingInput> = listed_inputs(&appearances, context.min_toi.0).into_iter().filter(|i| i.qualified).collect();
    let count = |p: Position| reference.iter().filter(|i| i.player.position == p).count();
    let mut per_game = game_ratings(context, &appearances, weights);
    let stats = compared_stats();
    let players = rankings::season_inputs(context, seasons)
        .into_iter()
        .filter(|i| i.side == Side::Ours)
        .map(|input| {
            let pool: Vec<&RatingInput> = reference.iter().filter(|r| r.player.position == input.player.position).collect();
            let rows: Vec<PlayerStatVsLeague> = stats
                .iter()
                .map(|&stat| {
                    let others: Vec<f64> = pool.iter().filter_map(|r| stat.value(r)).collect();
                    let value = stat.value(&input);
                    PlayerStatVsLeague {
                        stat,
                        league: mean(&others),
                        percentile: value.and_then(|v| percentile(v, &others, stat.higher_is_better())),
                        value,
                    }
                })
                .collect();
            let percentiles: Vec<f64> = rows.iter().filter_map(|r| r.percentile).collect();
            PlayerVsLeague {
                games: input.totals.games,
                toi: input.totals.toi.0,
                qualified: input.qualified,
                average_percentile: mean(&percentiles),
                game_ratings: per_game.remove(&input.player.id).unwrap_or_default(),
                stats: rows,
                player: input.player,
            }
        })
        .collect();
    (players, count(Position::Forward), count(Position::Defence))
}

#[must_use]
pub fn league(context: &Context<'_>, seasons: &[PlayerSeason], weights: &RatingWeights) -> LeagueReport {
    let games = team_games(context);
    let mut teams: Vec<String> = games.iter().filter(|g| !g.ours).map(|g| g.team.0.clone()).collect();
    teams.sort();
    teams.dedup();
    let (players, reference_forwards, reference_defence) = players_vs_league(context, seasons, weights);
    LeagueReport {
        team_name: context.scope.first().map(|g| g.team.0.clone()).unwrap_or_default(),
        loaded: context
            .league
            .iter()
            .map(|g| LeagueListing {
                id: g.id.clone(),
                date: g.date,
                teams: [g.sides[0].team.0.clone(), g.sides[1].team.0.clone()],
                score: (g.sides[0].goals, g.sides[1].goals),
                warnings: g.warnings.clone(),
            })
            .collect(),
        league_games: context.league.len(),
        reference_games: games.iter().filter(|g| !g.ours).count(),
        teams,
        reference_forwards,
        reference_defence,
        stats: team_stats(&games),
        games: games_vs_league(context, &games),
        standings: standings(&games),
        player_stats: compared_stats()
            .into_iter()
            .map(|s| StatInfo { stat: s, name: s.name().to_owned(), higher_is_better: s.higher_is_better() })
            .collect(),
        players,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_counts_teams_strictly_better() {
        assert_eq!(rank(50.0, &[60.0, 40.0, 50.0], true), 2);
        assert_eq!(rank(2.0, &[3.0, 1.0], false), 2);
        assert_eq!(rank(9.0, &[], true), 1);
    }

    #[test]
    fn percentile_is_fifty_in_the_middle_and_respects_direction() {
        let others = [1.0, 2.0, 3.0, 4.0];
        assert!((percentile(2.5, &others, true).unwrap() - 50.0).abs() < 1e-12);
        assert!((percentile(5.0, &others, true).unwrap() - 100.0).abs() < 1e-12);
        assert!((percentile(5.0, &others, false).unwrap() - 0.0).abs() < 1e-12);
        assert!((percentile(2.0, &others, true).unwrap() - 37.5).abs() < 1e-12);
        assert!(percentile(1.0, &[], true).is_none());
    }

    fn summary(regulation: [u32; 3]) -> TeamSummary {
        TeamSummary { shots_by_period: regulation.iter().map(|g| (10, 5, *g)).collect(), ..TeamSummary::default() }
    }

    #[test]
    fn a_league_loss_after_a_level_regulation_is_an_overtime_loss() {
        assert_eq!(league_outcome(2, 3, &summary([1, 1, 0]), &summary([0, 2, 0])), Outcome::OvertimeLoss);
        assert_eq!(league_outcome(1, 3, &summary([1, 0, 0]), &summary([1, 1, 1])), Outcome::Loss);
        assert_eq!(league_outcome(2, 2, &summary([1, 1, 0]), &summary([2, 0, 0])), Outcome::Tie);
    }

    #[test]
    fn pooled_rates_weigh_busy_games_more() {
        let team = TeamName("T".into());
        let other = TeamSummary::default();
        let (quiet, busy) = (TeamSummary { shots_on_goal: 10, ..TeamSummary::default() }, TeamSummary { shots_on_goal: 30, ..TeamSummary::default() });
        let side = |own, goals_for| TeamGame { team: &team, ours: false, own, other: &other, goals_for, goals_against: 0, outcome: Outcome::Win };
        let games = [side(&quiet, 1), side(&busy, 9)];
        let refs: Vec<&TeamGame<'_>> = games.iter().collect();
        let shooting = TEAM_STATS.iter().find(|s| s.name == "Shooting %").unwrap();
        assert!((shooting.measure.over(&refs).unwrap() - 25.0).abs() < 1e-12, "10 goals on 40 shots, not the 20% mean of 10% and 30%");
    }
}
