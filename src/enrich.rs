//! Adds what only InStat's PDF reports carry to a game built from the event export.
//!
//! That is its xG, InStat Index, goalie save splits, recent-game history, jersey numbers,
//! shot types and possession; everything the export has stays as the export has it.

use crate::error::Error;
use crate::model::{Game, Player, PlayerId, SkaterStats, TeamSummary};
use crate::parse::match_report::MatchReport;
use crate::parse::players_report::{PageKind, PlayerPage, PlayersReport};
use crate::reconcile::{goalie_stats, history, net_shots, page_details, reconcile};

/// Fills `game` in from whichever of the game's PDF reports were loaded.
pub fn add_reports(game: &mut Game, report: Option<&MatchReport>, players: Option<&PlayersReport>) -> Result<(), Error> {
    if let Some(report) = report {
        let pdf = reconcile(report, players)?;
        add_pdf_game(game, &pdf);
    } else if let Some(players) = players {
        add_player_pages(game, players);
    }
    Ok(())
}

fn same_surname(a: &str, b: &str) -> bool {
    a.to_uppercase() == b.to_uppercase()
}

/// The player with `id` or, failing that, the only one with `surname`.
fn matching<'a>(players: &'a mut [Player], id: &PlayerId, surname: &str) -> Option<&'a mut Player> {
    if let Some(index) = players.iter().position(|p| &p.id == id) {
        return players.get_mut(index);
    }
    let mut same: Vec<usize> = players.iter().enumerate().filter(|(_, p)| same_surname(&p.surname, surname)).map(|(i, _)| i).collect();
    match (same.pop(), same.is_empty()) {
        (Some(index), true) => players.get_mut(index),
        _ => None,
    }
}

fn add_instat_skater_numbers(stats: &mut SkaterStats, from: &SkaterStats) {
    stats.instat_index = from.instat_index.or(stats.instat_index);
    stats.xg = from.xg.or(stats.xg);
    stats.on_ice_xg_for = from.on_ice_xg_for.or(stats.on_ice_xg_for);
    stats.on_ice_xg_against = from.on_ice_xg_against.or(stats.on_ice_xg_against);
    stats.hits_against = from.hits_against;
    stats.shot_sources.positional_attack = from.shot_sources.positional_attack;
    stats.shot_sources.counter_attack = from.shot_sources.counter_attack;
    if !from.shot_types.is_empty() {
        stats.shot_types.clone_from(&from.shot_types);
    }
    if !from.net_shots.is_empty() {
        stats.net_shots.clone_from(&from.net_shots);
    }
}

fn add_possession_and_xg(summary: &mut TeamSummary, from: &TeamSummary) {
    summary.xg = from.xg.or(summary.xg);
    summary.possession_time = from.possession_time;
    summary.possession_pct = from.possession_pct;
    summary.possession_pct_by_period.clone_from(&from.possession_pct_by_period);
}

fn add_pdf_game(game: &mut Game, pdf: &Game) {
    let mut unmatched = Vec::new();
    for from in &pdf.players {
        let Some(player) = matching(&mut game.players, &from.id, &from.surname) else {
            unmatched.push(from.name.clone());
            continue;
        };
        player.jersey = from.jersey.or(player.jersey);
        if !from.history.is_empty() {
            player.history.clone_from(&from.history);
        }
        player.details.extend(from.details.iter().filter(|d| d.group == "Player report").cloned());
        if let (Some(stats), Some(from)) = (player.skater.as_mut(), from.skater.as_ref()) {
            add_instat_skater_numbers(stats, from);
        }
        if let (Some(stats), Some(from)) = (player.goalie.as_mut(), from.goalie.as_ref()) {
            stats.instat_index = from.instat_index.or(stats.instat_index);
            stats.splits = from.splits.clone();
            stats.rebounds = from.rebounds.or(stats.rebounds);
        }
    }
    add_possession_and_xg(&mut game.summary, &pdf.summary);
    add_possession_and_xg(&mut game.opponent_summary, &pdf.opponent_summary);
    warn_unmatched(game, &unmatched);
}

fn page_decimal(page: &PlayerPage, label: &str) -> Option<f64> {
    page.stat(label).and_then(|c| c.decimal())
}

fn add_player_pages(game: &mut Game, report: &PlayersReport) {
    let mut unmatched = Vec::new();
    let date = game.date;
    let team = game.team.clone();
    for page in &report.players {
        let id = PlayerId::new(&team, &page.full_name);
        let surname = page.full_name.split_whitespace().last().unwrap_or_default();
        let Some(player) = matching(&mut game.players, &id, surname) else {
            unmatched.push(page.full_name.clone());
            continue;
        };
        player.jersey = page.jersey.or(player.jersey);
        player.history = history(page, date);
        player.details.extend(page_details(page));
        match (page.kind, player.skater.as_mut(), player.goalie.as_mut()) {
            (PageKind::Skater, Some(stats), _) => {
                stats.instat_index = page_decimal(page, "InStat Index").or(stats.instat_index);
                stats.xg = page_decimal(page, "xG");
                stats.on_ice_xg_for = page_decimal(page, "Team xG when on ice");
                stats.on_ice_xg_against = page_decimal(page, "Opponent's xG when on ice");
                stats.net_shots = net_shots(&page.net);
            }
            (PageKind::Goalie, _, Some(stats)) => {
                let from = goalie_stats(page);
                stats.instat_index = from.instat_index;
                stats.splits = from.splits;
                stats.rebounds = from.rebounds;
            }
            _ => unmatched.push(page.full_name.clone()),
        }
    }
    warn_unmatched(game, &unmatched);
}

fn warn_unmatched(game: &mut Game, unmatched: &[String]) {
    if !unmatched.is_empty() {
        game.warnings.push(format!("in the PDF reports but not matched to the event export: {}", unmatched.join(", ")));
    }
}
