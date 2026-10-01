//! Adds what only InStat's PDF reports carry to a game built from the event export.
//!
//! That is its xG, InStat Index, goalie save splits, recent-game history, jersey numbers,
//! shot types, kinds of attack, hits taken and possession; everything the export has stays
//! as the export has it.

use std::collections::HashMap;

use crate::cell::Cell;
use crate::model::{
    BodyArea, CellValue, Date, Game, GoaliePageRow, GoalieState, HistoryKind, HistoryRow, NetArea, NetShots, Player, PlayerId,
    ReboundControl, SaveSplits, Saves, Seconds, ShotDistance, ShotSituation, ShotType, SkaterStats, StatEntry, Tally, TeamSummary,
    TypeShots,
};
use crate::parse::common::{PlayerRow, RowLabel};
use crate::parse::match_report::MatchReport;
use crate::parse::players_report::{HistoryColumn, PageKind, PlayerPage, PlayersReport, infer_year};
use crate::parse::team_stats::{InstatTeamNumbers, instat_numbers};

/// Players who share a surname are told apart by ice time to within this many seconds; the
/// export and the report round it differently.
const TOI_TOLERANCE: f64 = 5.0;

/// Fills `game` in from whichever of the game's PDF reports were loaded.
pub fn add_reports(game: &mut Game, report: Option<&MatchReport>, players: Option<&PlayersReport>) {
    if let Some(players) = players {
        add_player_pages(game, players);
    }
    if let Some(report) = report {
        add_match_report(game, report);
    }
}

/// Whether `player` has `surname`, which may be several words ("Van Horn").
fn has_surname(player: &Player, surname: &str) -> bool {
    let (name, surname) = (player.name.to_uppercase(), surname.trim().to_uppercase());
    name.ends_with(&format!(" {surname}")) || player.surname.to_uppercase() == surname
}

/// The only player in `players` with `surname`.
fn by_surname<'a>(players: &'a mut [Player], surname: &str) -> Option<&'a mut Player> {
    let mut matches = players.iter_mut().filter(|p| has_surname(p, surname));
    let found = matches.next();
    if matches.next().is_some() { None } else { found }
}

/// The player with `id` or, failing that, the only one with `surname`.
fn by_id_or_surname<'a>(players: &'a mut [Player], id: &PlayerId, surname: &str) -> Option<&'a mut Player> {
    match players.iter().position(|p| &p.id == id) {
        Some(index) => players.get_mut(index),
        None => by_surname(players, surname),
    }
}

fn warn_unmatched(game: &mut Game, report: &str, unmatched: &[String]) {
    if !unmatched.is_empty() {
        game.warnings.push(format!("in the {report} but not matched to the event export: {}", unmatched.join(", ")));
    }
}

fn add_instat_numbers(summary: &mut TeamSummary, numbers: InstatTeamNumbers) {
    summary.xg = numbers.xg;
    summary.possession_time = numbers.possession_time;
    summary.possession_pct = numbers.possession_pct;
    summary.possession_pct_by_period = numbers.possession_pct_by_period;
}

fn tally(cell: &Cell) -> Tally {
    let (total, succeeded) = cell.ratio().unwrap_or_default();
    Tally { total, succeeded }
}

fn column(row: &PlayerRow, name: &str) -> Cell {
    row.get_exact(name).map_or(Cell::Empty, Cell::parse)
}

fn shot_types(row: &PlayerRow) -> Vec<TypeShots> {
    ShotType::ALL
        .iter()
        .filter_map(|&kind| {
            let label = kind.shots_table_label()?;
            Some(TypeShots { kind, shots: tally(&Cell::parse(row.get_exact(label)?)) })
        })
        .collect()
}

/// The player a main-table row is: the only one with its surname or, when several share
/// it, the one whose ice time matches.
fn row_player(players: &[Player], row: &PlayerRow) -> Option<PlayerId> {
    let named: Vec<&Player> = players.iter().filter(|p| has_surname(p, &row.label.surname)).collect();
    if let [only] = named.as_slice() {
        return Some(only.id.clone());
    }
    let toi = f64::from(column(row, "Time on ice").seconds()?);
    named
        .into_iter()
        .filter_map(|p| Some((p, (p.skater.as_ref()?.toi.0 - toi).abs())))
        .filter(|(_, gap)| *gap <= TOI_TOLERANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p.id.clone())
}

fn skater_mut<'a>(game: &'a mut Game, id: &PlayerId) -> Option<&'a mut SkaterStats> {
    game.players.iter_mut().find(|p| &p.id == id).and_then(|p| p.skater.as_mut())
}

fn add_match_report(game: &mut Game, report: &MatchReport) {
    let ours = report.our_index;
    add_instat_numbers(&mut game.summary, instat_numbers(&report.team_stats.entries[ours]));
    add_instat_numbers(&mut game.opponent_summary, instat_numbers(&report.team_stats.entries[1 - ours]));
    let mut unmatched = Vec::new();
    let mut by_label: HashMap<&RowLabel, PlayerId> = HashMap::new();
    for row in &report.players {
        let Some(id) = row_player(&game.players, row) else {
            unmatched.push(row.label.surname.clone());
            continue;
        };
        if let Some(stats) = skater_mut(game, &id) {
            stats.hits_against = column(row, "Hits against").count().unwrap_or_default();
        }
        by_label.insert(&row.label, id);
    }
    for row in &report.shots {
        let Some(stats) = by_label.get(&row.label).and_then(|id| skater_mut(game, id)) else {
            continue;
        };
        stats.shot_types = shot_types(row);
        stats.shot_sources.positional_attack = tally(&column(row, "In positional attacks"));
        stats.shot_sources.counter_attack = tally(&column(row, "In counter-attacks"));
    }
    warn_unmatched(game, "match report", &unmatched);
}

fn page_decimal(page: &PlayerPage, label: &str) -> Option<f64> {
    page.stat(label).and_then(|c| c.decimal())
}

fn add_player_pages(game: &mut Game, report: &PlayersReport) {
    let mut unmatched = Vec::new();
    let (date, team) = (game.date, game.team.clone());
    for page in &report.players {
        let id = PlayerId::new(&team, &page.full_name);
        let surname = page.full_name.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
        let Some(player) = by_id_or_surname(&mut game.players, &id, &surname) else {
            unmatched.push(page.full_name.clone());
            continue;
        };
        player.jersey = page.jersey.or(player.jersey);
        player.history = history(page, date);
        player.details = page_details(page);
        match (page.kind, player.skater.as_mut(), player.goalie.as_mut()) {
            (PageKind::Skater, Some(stats), _) => {
                stats.instat_index = page_decimal(page, "InStat Index");
                stats.xg = page_decimal(page, "xG");
                stats.on_ice_xg_for = page_decimal(page, "Team xG when on ice");
                stats.on_ice_xg_against = page_decimal(page, "Opponent's xG when on ice");
                stats.net_shots = net_shots(&page.net);
            }
            (PageKind::Goalie, _, Some(stats)) => {
                stats.instat_index = page_decimal(page, "InStat Index");
                stats.splits = save_splits(page);
                stats.rebounds = rebound_control(page);
            }
            _ => unmatched.push(page.full_name.clone()),
        }
    }
    warn_unmatched(game, "player report", &unmatched);
}

fn net_shots(counts: &[(NetArea, (u32, u32))]) -> Vec<NetShots> {
    counts.iter().map(|&(area, (on_goal, goals))| NetShots { area, on_goal, goals }).collect()
}

/// One `Saves` per category whose row the goalie page printed.
fn page_saves<K: GoaliePageRow>(page: &PlayerPage) -> Vec<Saves<K>> {
    K::ALL
        .iter()
        .filter_map(|&kind| {
            let (shots, saves) = page.stat(kind.goalie_label())?.ratio()?;
            Some(Saves { kind, shots, saves })
        })
        .collect()
}

fn diagram_saves<K: Copy>(counts: &[(K, (u32, u32))]) -> Vec<Saves<K>> {
    counts.iter().map(|&(kind, (shots, saves))| Saves { kind, shots, saves }).collect()
}

fn save_splits(page: &PlayerPage) -> SaveSplits {
    SaveSplits {
        distance: page_saves::<ShotDistance>(page),
        shot_type: page_saves::<ShotType>(page),
        situation: page_saves::<ShotSituation>(page),
        goalie_state: page_saves::<GoalieState>(page),
        body_area: page_saves::<BodyArea>(page),
        net_area: diagram_saves(&page.net),
        zone: diagram_saves(&page.zones),
    }
}

fn rebound_control(page: &PlayerPage) -> Option<ReboundControl> {
    let count = |label: &str| page.stat(label).and_then(|c| c.count());
    Some(ReboundControl {
        uncontrolled: count("Uncontrolled rebound")?,
        controlled: count("Controlled rebound")?,
        frozen_after_rebound: count("Freezing the puck after rebound")?,
        frozen_immediately: count("Freezing the puck straight away")?,
    })
}

fn history(page: &PlayerPage, reference: Date) -> Vec<HistoryRow> {
    page.history
        .iter()
        .map(|row| {
            let get = |column: HistoryColumn| row.cells.iter().find(|(c, _)| *c == column).map_or(Cell::Empty, |(_, cell)| cell.clone());
            let count = |column| get(column).count().unwrap_or_default();
            let kind = match page.kind {
                PageKind::Skater => {
                    let (shots, shots_on_goal) = get(HistoryColumn::Shots).ratio().unwrap_or_default();
                    HistoryKind::Skater {
                        goals: count(HistoryColumn::Goals),
                        assists: count(HistoryColumn::Assists),
                        shots,
                        shots_on_goal,
                        plus_minus: get(HistoryColumn::PlusMinus).signed().unwrap_or_default(),
                    }
                }
                PageKind::Goalie => HistoryKind::Goalie {
                    shots_against: count(HistoryColumn::ShotsAgainst),
                    saves: count(HistoryColumn::Saves),
                    goals_against: count(HistoryColumn::GoalsAgainst),
                },
            };
            HistoryRow {
                date: infer_year(row.day, row.month, reference),
                opponent: row.opponent.clone(),
                instat_index: match get(HistoryColumn::InstatIndex) {
                    Cell::Empty => None,
                    other => other.decimal(),
                },
                toi: Seconds(f64::from(get(HistoryColumn::TimeOnIce).seconds().unwrap_or_default())),
                kind,
            }
        })
        .collect()
}

fn page_details(page: &PlayerPage) -> Vec<StatEntry> {
    page.stats
        .iter()
        .map(|(label, game, _)| StatEntry { group: "Player report".to_owned(), label: label.clone(), value: CellValue::from_text(game) })
        .collect()
}
