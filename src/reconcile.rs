//! Joins a parsed match report (and optionally the player report) into one [`Game`].
//!
//! The match report's per-player tables print unreliable jersey numbers, so rows are tied to
//! players by surname, falling back to time on ice when two players share a surname.

use std::collections::HashMap;

use crate::cell::Cell;
use crate::error::Error;
use crate::model::{
    Advantage, AreaBattles, BattleArea, CellValue, DistanceSaves, Game, GameId, Goal, GoalieStats, HistoryKind, HistoryRow, Interval,
    Jersey, Player, PlayerId, PlayerMatrix, Position, Seconds, ShotZone, SkaterStats, StatEntry,
    Strength, Team, TeamName, TeamStatRow, Unit, UnitKind, ZoneShots, add_zone_shots,
};
use crate::parse::common::{PlayerRow, RowLabel};
use crate::parse::match_report::MatchReport;
use crate::parse::players_report::{
    HistoryColumn, PageKind, PlayerPage, PlayersReport, infer_year,
};
use crate::parse::team_stats;
use crate::parse::timeline::{GoalMarker, Timeline, TimelineRow};

/// InStat draws goal markers slightly after the whistle; shift ends within this window
/// before the marker are taken as the goal time.
const GOAL_SNAP_BEFORE: f64 = 2.5;
const GOAL_SNAP_AFTER: f64 = 0.5;
/// Typical marker offset when no shift ends near it.
const GOAL_MARKER_LAG: f64 = 0.7;
/// On-ice sets are sampled this long before the goal, so a line change at the whistle
/// counts the players who were on for the goal.
const ON_ICE_PROBE: f64 = 0.25;
const TOI_MATCH_TOLERANCE: f64 = 5.0;

#[derive(Debug, Clone)]
struct RosterEntry {
    id: PlayerId,
    name: String,
    jersey: Option<Jersey>,
    page: Option<PlayerPage>,
}

impl RosterEntry {
    fn matches_surname(&self, surname: &str) -> bool {
        let name = self.name.to_uppercase();
        let surname = surname.to_uppercase();
        name == surname || name.ends_with(&format!(" {surname}"))
    }

    fn page_toi(&self) -> Option<f64> {
        let page = self.page.as_ref()?;
        page.stat("Time on ice")
            .and_then(|c| c.seconds())
            .map(f64::from)
    }
}

struct Resolver<'a> {
    roster: &'a [RosterEntry],
    /// Match-report main-table rows (index, label with buggy number) with resolved ids.
    table_rows: Vec<(usize, RowLabel, PlayerId)>,
}

impl Resolver<'_> {
    fn by_surname_and_toi(&self, surname: &str, toi: Option<f64>) -> Option<PlayerId> {
        let candidates: Vec<&RosterEntry> = self
            .roster
            .iter()
            .filter(|r| r.page.as_ref().is_none_or(|p| p.kind == PageKind::Skater))
            .filter(|r| r.matches_surname(surname))
            .collect();
        match candidates.as_slice() {
            [only] => Some(only.id.clone()),
            [] => None,
            many => {
                let toi = toi?;
                many.iter()
                    .filter_map(|r| r.page_toi().map(|t| ((t - toi).abs(), r)))
                    .filter(|(diff, _)| *diff <= TOI_MATCH_TOLERANCE)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, r)| r.id.clone())
            }
        }
    }

    /// Timeline rows use the same (buggy) labels as the main table.
    fn by_table_label(&self, label: &RowLabel, toi: Option<f64>) -> Option<PlayerId> {
        let exact: Vec<&PlayerId> = self
            .table_rows
            .iter()
            .filter(|(_, l, _)| l == label)
            .map(|(_, _, id)| id)
            .collect();
        if let [only] = exact.as_slice() {
            return Some((*only).clone());
        }
        self.by_surname_and_toi(&label.surname, toi)
    }

    /// Lines-page labels carry real jersey numbers.
    fn by_jersey(&self, label: &RowLabel) -> Option<PlayerId> {
        let with_jersey: Vec<&RosterEntry> = self
            .roster
            .iter()
            .filter(|r| r.matches_surname(&label.surname))
            .filter(|r| label.number.is_some_and(|n| r.jersey == Some(Jersey(n))))
            .collect();
        if let [only] = with_jersey.as_slice() {
            return Some(only.id.clone());
        }
        self.by_surname_and_toi(&label.surname, None)
    }
}

fn surname_of(full_name: &str) -> String {
    full_name
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_roster(
    team: &TeamName,
    players: Option<&PlayersReport>,
    main: &[PlayerRow],
) -> Vec<RosterEntry> {
    if let Some(report) = players {
        return report
            .players
            .iter()
            .map(|page| RosterEntry {
                id: PlayerId::new(team, &page.full_name),
                name: page.full_name.clone(),
                jersey: page.jersey,
                page: Some(page.clone()),
            })
            .collect();
    }
    let mut roster: Vec<RosterEntry> = Vec::new();
    for row in main {
        let duplicate = main
            .iter()
            .filter(|r| r.label.surname == row.label.surname)
            .count()
            > 1;
        let name = if duplicate {
            format!("{} ({})", row.label.surname, roster.len() + 1)
        } else {
            row.label.surname.clone()
        };
        roster.push(RosterEntry {
            id: PlayerId::new(team, &name),
            name,
            jersey: None,
            page: None,
        });
    }
    roster
}

fn cell(row: &PlayerRow, prefix: &str) -> Cell {
    row.get(prefix).map_or(Cell::Empty, Cell::parse)
}

fn exact_cell(row: &PlayerRow, name: &str) -> Cell {
    row.get_exact(name).map_or(Cell::Empty, Cell::parse)
}

fn seconds(c: &Cell) -> Seconds {
    Seconds(f64::from(c.seconds().unwrap_or_default()))
}

fn skater_stats(
    main: &PlayerRow,
    extra: &[Option<&PlayerRow>],
    page: Option<&PlayerPage>,
    passes: u32,
) -> SkaterStats {
    let find = |prefix: &str| {
        extra
            .iter()
            .flatten()
            .find_map(|row| row.get(prefix).map(Cell::parse))
            .unwrap_or(Cell::Empty)
    };
    let (shots, shots_on_goal) = cell(main, "Shots / shots on goal")
        .ratio()
        .unwrap_or_default();
    let (faceoffs, faceoffs_won) = cell(main, "Faceoffs / won").ratio().unwrap_or_default();
    let (puck_battles, puck_battles_won) = find("Puck battles / won").ratio().unwrap_or_default();
    let page_decimal = |label: &str| page.and_then(|p| p.stat(label)).and_then(|c| c.decimal());
    SkaterStats {
        instat_index: cell(main, "InStat Index").decimal(),
        goals: cell(main, "Goals").count().unwrap_or_default(),
        assists: cell(main, "Assists").count().unwrap_or_default(),
        plus_minus: cell(main, "+/-").signed().unwrap_or_default(),
        toi: seconds(&cell(main, "Time on ice")),
        shifts: cell(main, "Shifts").count().unwrap_or_default(),
        pp_toi: seconds(&cell(main, "Power play time")),
        sh_toi: seconds(&cell(main, "Short-handed time")),
        penalty_minutes: seconds(&cell(main, "Penalty time")),
        shots,
        shots_on_goal,
        corsi_for: exact_cell(main, "CORSI+").count().unwrap_or_default(),
        corsi_against: exact_cell(main, "CORSI-").count().unwrap_or_default(),
        hits: exact_cell(main, "Hits").count().unwrap_or_default(),
        hits_against: exact_cell(main, "Hits against").count().unwrap_or_default(),
        faceoffs,
        faceoffs_won,
        blocked_shots: cell(main, "Shots blocking").count().unwrap_or_default(),
        puck_battles,
        puck_battles_won,
        puck_losses: find("Puck losses").count().unwrap_or_default(),
        puck_recoveries: find("Puck recoveries").count().unwrap_or_default(),
        entries: extra
            .iter()
            .flatten()
            .find_map(|row| row.get_exact("Entries").map(Cell::parse))
            .and_then(|c| c.count())
            .unwrap_or_default(),
        passes,
        xg: page_decimal("xG"),
        on_ice_xg_for: page_decimal("Team xG when on ice"),
        on_ice_xg_against: page_decimal("Opponent's xG when on ice"),
        shot_zones: extra.get(SHOTS_TABLE).copied().flatten().map(shot_zones).unwrap_or_default(),
        battle_areas: extra.get(BATTLES_TABLE).copied().flatten().map(battle_areas).unwrap_or_default(),
    }
}

/// Indexes of tables among the extra tables passed to [`skater_stats`].
const SHOTS_TABLE: usize = 3;
const BATTLES_TABLE: usize = 4;

#[derive(Clone, Copy)]
enum Half {
    Ours,
    Theirs,
}

/// Columns follow a zone header, so "In corners" belongs to whichever zone came last.
fn battle_area(label: &str, half: Option<Half>) -> Option<BattleArea> {
    Some(match (label, half) {
        ("Own slot", _) => BattleArea::OwnSlot,
        ("Behind own goal", _) => BattleArea::BehindOwnGoal,
        ("In corners", Some(Half::Ours)) => BattleArea::OwnCorners,
        ("In corners", Some(Half::Theirs)) => BattleArea::OppCorners,
        ("Own blue line", _) => BattleArea::OwnBlueLine,
        ("Neutral zone", _) => BattleArea::NeutralZone,
        ("Opp blue line" | "Opp. blue line", _) => BattleArea::OppBlueLine,
        ("Behind opp. goal" | "Behind opp goal", _) => BattleArea::BehindOppGoal,
        ("Opp. slot" | "Opp slot", _) => BattleArea::OppSlot,
        _ => return None,
    })
}

fn battle_areas(row: &PlayerRow) -> Vec<AreaBattles> {
    let mut half = None;
    let mut areas = Vec::new();
    for (label, value) in &row.cells {
        match label.as_str() {
            "Defensive zone" => half = Some(Half::Ours),
            "Offensive zone" => half = Some(Half::Theirs),
            _ => {}
        }
        let Some(area) = battle_area(label, half) else {
            continue;
        };
        if let Some((battles, won)) = Cell::parse(value).ratio() {
            areas.push(AreaBattles { area, battles, won });
        }
    }
    areas.sort_by_key(|a| a.area);
    areas
}

/// Sums shots by zone over the opponent's shots table.
fn shot_zones_against(rows: &[PlayerRow]) -> Vec<ZoneShots> {
    let mut totals = Vec::new();
    for row in rows {
        add_zone_shots(&mut totals, &shot_zones(row));
    }
    totals
}

fn shot_zones(row: &PlayerRow) -> Vec<ZoneShots> {
    ShotZone::ALL
        .iter()
        .filter_map(|&zone| {
            let (shots, on_goal) = row.get_exact(zone.instat_label()).map(Cell::parse)?.ratio()?;
            Some(ZoneShots { zone, shots, on_goal })
        })
        .collect()
}

const GOALIE_DISTANCE_BANDS: [&str; 4] = ["From the slot", "From close range", "From midrange", "From long range distance"];

fn goalie_stats(page: &PlayerPage) -> GoalieStats {
    let ratio = |label: &str| page.stat(label).and_then(|c| c.ratio());
    let (shots_against, saves) = ratio("Shots against / saved").unwrap_or_default();
    GoalieStats {
        instat_index: page.stat("InStat Index").and_then(|c| c.decimal()),
        toi: Seconds(f64::from(
            page.stat("Time on ice")
                .and_then(|c| c.seconds())
                .unwrap_or_default(),
        )),
        shots_against,
        saves,
        goals_against: page
            .stat("Goals against")
            .and_then(|c| c.count())
            .unwrap_or_default(),
        even_strength: ratio("At even strength"),
        short_handed: ratio("Short-handed"),
        by_distance: GOALIE_DISTANCE_BANDS
            .iter()
            .filter_map(|band| {
                let (shots, saves) = ratio(band)?;
                Some(DistanceSaves { band: (*band).to_owned(), shots, saves })
            })
            .collect(),
    }
}

fn history(page: &PlayerPage, reference: crate::model::Date) -> Vec<HistoryRow> {
    page.history
        .iter()
        .map(|row| {
            let get = |column: HistoryColumn| {
                row.cells
                    .iter()
                    .find(|(c, _)| *c == column)
                    .map_or(Cell::Empty, |(_, cell)| cell.clone())
            };
            let count = |column| get(column).count().unwrap_or_default();
            let kind = match page.kind {
                PageKind::Skater => {
                    let (shots, shots_on_goal) =
                        get(HistoryColumn::Shots).ratio().unwrap_or_default();
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
            let instat = get(HistoryColumn::InstatIndex);
            HistoryRow {
                date: infer_year(row.day, row.month, reference),
                opponent: row.opponent.clone(),
                instat_index: match instat {
                    Cell::Empty => None,
                    other => other.decimal(),
                },
                toi: Seconds(f64::from(
                    get(HistoryColumn::TimeOnIce).seconds().unwrap_or_default(),
                )),
                kind,
            }
        })
        .collect()
}

fn page_details(page: &PlayerPage) -> Vec<StatEntry> {
    page.stats
        .iter()
        .map(|(label, game, _)| StatEntry {
            group: "Player report".to_owned(),
            label: label.clone(),
            value: CellValue::from_text(game),
        })
        .collect()
}

/// Rows of a secondary table, aligned to the main table by label sequence.
fn aligned<'a>(
    main: &[PlayerRow],
    table: &'a [PlayerRow],
    name: &str,
    warnings: &mut Vec<String>,
) -> Vec<Option<&'a PlayerRow>> {
    let same_order =
        main.len() == table.len() && main.iter().zip(table).all(|(a, b)| a.label == b.label);
    if same_order {
        return table.iter().map(Some).collect();
    }
    main.iter()
        .map(|row| {
            let matches: Vec<&PlayerRow> = table.iter().filter(|t| t.label == row.label).collect();
            if let [only] = matches.as_slice() {
                Some(*only)
            } else {
                warnings.push(format!(
                    "{name}: could not match row for {}",
                    row.label.surname
                ));
                None
            }
        })
        .collect()
}

fn position_from_units(units: &[Unit], id: &PlayerId) -> Option<Position> {
    let toi_in = |kind: UnitKind| -> f64 {
        units
            .iter()
            .filter(|u| u.kind == kind && u.players.contains(id))
            .map(|u| u.toi.0)
            .sum()
    };
    let defence = toi_in(UnitKind::DefencePair);
    let forward = toi_in(UnitKind::ForwardLine);
    if defence == 0.0 && forward == 0.0 {
        None
    } else if defence >= forward {
        Some(Position::Defence)
    } else {
        Some(Position::Forward)
    }
}

/// Everything known about one roster player's skating line in the match report.
struct SkaterSources<'a> {
    main: &'a PlayerRow,
    extras: Vec<Option<&'a PlayerRow>>,
}

const DETAIL_GROUPS: [&str; 6] = [
    "Main",
    "Challenges",
    "Turnovers",
    "Entries",
    "Shots",
    "Challenges by zone",
];

fn skater_sources<'a>(
    tables: &'a crate::parse::match_report::PlayerTables,
    warnings: &mut Vec<String>,
) -> Vec<SkaterSources<'a>> {
    let main = &tables.main;
    let extras = [
        aligned(main, &tables.challenges, "challenges", warnings),
        aligned(main, &tables.turnovers, "turnovers", warnings),
        aligned(main, &tables.entries, "entries", warnings),
        aligned(main, &tables.shots, "shots", warnings),
        aligned(
            main,
            &tables.challenges_by_zone,
            "challenges by zone",
            warnings,
        ),
    ];
    main.iter()
        .enumerate()
        .map(|(i, row)| SkaterSources {
            main: row,
            extras: extras
                .iter()
                .map(|table| table.get(i).copied().flatten())
                .collect(),
        })
        .collect()
}

fn skater_details(sources: &SkaterSources<'_>) -> Vec<StatEntry> {
    let mut details = sources.main.entries(DETAIL_GROUPS[0]);
    for (row, group) in sources.extras.iter().zip(&DETAIL_GROUPS[1..]) {
        if let Some(row) = row {
            details.extend(row.entries(group));
        }
    }
    details
}

struct PlayerContext<'a> {
    sources: Vec<SkaterSources<'a>>,
    resolver: &'a Resolver<'a>,
    units: &'a [Unit],
    shifts: &'a HashMap<PlayerId, Vec<Interval>>,
    groups: HashMap<PlayerId, String>,
    pass_totals: HashMap<PlayerId, u32>,
    date: crate::model::Date,
}

fn build_player(entry: &RosterEntry, context: &PlayerContext<'_>) -> Option<Player> {
    let row_index = context
        .resolver
        .table_rows
        .iter()
        .find(|(_, _, id)| id == &entry.id)
        .map(|(index, _, _)| *index);
    let is_goalie = entry
        .page
        .as_ref()
        .is_some_and(|p| p.kind == PageKind::Goalie);
    let sources = row_index.and_then(|i| context.sources.get(i));
    if sources.is_none() && !is_goalie {
        return None;
    }
    let skater = sources.map(|s| {
        skater_stats(
            s.main,
            &s.extras,
            entry.page.as_ref(),
            context
                .pass_totals
                .get(&entry.id)
                .copied()
                .unwrap_or_default(),
        )
    });
    let mut details = sources.map(skater_details).unwrap_or_default();
    if let Some(page) = &entry.page {
        details.extend(page_details(page));
    }
    let position = if is_goalie {
        Position::Goalie
    } else {
        position_from_units(context.units, &entry.id).unwrap_or_else(|| {
            if skater.as_ref().is_some_and(|s| s.faceoffs > 0) {
                Position::Forward
            } else {
                Position::Unknown
            }
        })
    };
    Some(Player {
        id: entry.id.clone(),
        name: entry.name.clone(),
        surname: surname_of(&entry.name),
        jersey: entry.jersey,
        position,
        group: context.groups.get(&entry.id).cloned(),
        skater,
        goalie: entry.page.as_ref().filter(|_| is_goalie).map(goalie_stats),
        shifts: context.shifts.get(&entry.id).cloned().unwrap_or_default(),
        history: entry
            .page
            .as_ref()
            .map(|p| history(p, context.date))
            .unwrap_or_default(),
        details,
    })
}

fn timeline_groups(
    timeline: Option<&Timeline>,
    resolver: &Resolver<'_>,
) -> HashMap<PlayerId, String> {
    timeline
        .iter()
        .flat_map(|t| t.rows.iter())
        .filter_map(|row| {
            let toi = row.shifts.iter().map(|s| s.duration().0).sum::<f64>();
            Some((
                resolver.by_table_label(&row.label, Some(toi))?,
                row.group.clone()?,
            ))
        })
        .collect()
}

fn pass_totals(passes: Option<&PlayerMatrix>) -> HashMap<PlayerId, u32> {
    passes
        .map(|m| {
            m.players
                .iter()
                .zip(&m.values)
                .map(|(id, row)| (id.clone(), row.iter().sum()))
                .collect()
        })
        .unwrap_or_default()
}

struct Events {
    goals: Vec<Goal>,
    advantages: Vec<Advantage>,
    length: Seconds,
}

fn events(
    timeline: Option<&Timeline>,
    ours: usize,
    shifts: &HashMap<PlayerId, Vec<Interval>>,
    warnings: &mut Vec<String>,
) -> Events {
    let Some(timeline) = timeline else {
        warnings.push("no game time distribution page: shifts and goal timing unavailable".into());
        return Events {
            goals: Vec::new(),
            advantages: Vec::new(),
            length: Seconds(3.0 * crate::model::PERIOD_SECONDS),
        };
    };
    let advantages: Vec<Advantage> = timeline
        .bands
        .iter()
        .map(|band| Advantage {
            interval: band.interval,
            team: if band.strength == Strength::PowerPlay {
                Team::Us
            } else {
                Team::Them
            },
        })
        .collect();
    let goals = resolve_goals(timeline, ours, &advantages, shifts);
    Events {
        goals,
        advantages,
        length: timeline.length,
    }
}

fn team_stat_rows(report: &MatchReport) -> Vec<TeamStatRow> {
    let ours = report.our_index;
    let entries = &report.team_stats.entries;
    entries[ours]
        .iter()
        .zip(&entries[1 - ours])
        .map(|(a, b)| TeamStatRow {
            group: a.group.clone(),
            label: a.label.clone(),
            ours: a.value.clone(),
            theirs: b.value.clone(),
        })
        .collect()
}

fn check_goal_count(
    goals: &[Goal],
    goals_for: u32,
    goals_against: u32,
    warnings: &mut Vec<String>,
) {
    if !goals.is_empty() && u32::try_from(goals.len()).ok() != Some(goals_for + goals_against) {
        warnings.push(format!(
            "timeline shows {} goals but the score is {goals_for}-{goals_against}",
            goals.len()
        ));
    }
}

fn resolve_table_rows(main: &[PlayerRow], resolver: &mut Resolver<'_>, warnings: &mut Vec<String>) {
    for (index, row) in main.iter().enumerate() {
        let toi = cell(row, "Time on ice").seconds().map(f64::from);
        match resolver.by_surname_and_toi(&row.label.surname, toi) {
            Some(id) => resolver.table_rows.push((index, row.label.clone(), id)),
            None => warnings.push(format!("no roster match for {}", row.label.surname)),
        }
    }
}

pub fn reconcile(report: &MatchReport, players: Option<&PlayersReport>) -> Result<Game, Error> {
    let mut warnings = Vec::new();
    let title = &report.title;
    let ours = report.our_index;
    let team = title.teams[ours].clone();
    let opponent = title.teams[1 - ours].clone();
    let tables = &report.ours.tables;
    if tables.main.is_empty() {
        return Err(Error::parse(
            "reconcile",
            "match report has no player table for our team",
        ));
    }

    let roster = build_roster(&team, players, &tables.main);
    let mut resolver = Resolver {
        roster: &roster,
        table_rows: Vec::new(),
    };
    resolve_table_rows(&tables.main, &mut resolver, &mut warnings);
    let timeline = report.ours.timeline.as_ref();
    let units = resolve_units(report, &resolver, &mut warnings);
    let shifts = resolve_shifts(timeline, &resolver, &mut warnings);
    let passes = report
        .ours
        .passes
        .as_ref()
        .and_then(|m| pass_matrix(m, &resolver, &mut warnings));

    let context = PlayerContext {
        sources: skater_sources(tables, &mut warnings),
        resolver: &resolver,
        units: &units,
        shifts: &shifts,
        groups: timeline_groups(timeline, &resolver),
        pass_totals: pass_totals(passes.as_ref()),
        date: title.date,
    };
    let players_out: Vec<Player> = roster
        .iter()
        .filter_map(|entry| build_player(entry, &context))
        .collect();

    let Events {
        goals,
        advantages,
        length,
    } = events(timeline, ours, &shifts, &mut warnings);
    let (goals_for, goals_against) = if ours == 0 {
        title.score
    } else {
        (title.score.1, title.score.0)
    };
    check_goal_count(&goals, goals_for, goals_against, &mut warnings);
    check_plus_minus(&players_out, &goals, &mut warnings);

    Ok(Game {
        id: GameId(format!(
            "{}_{}_{}-{}",
            title.date,
            slug(&opponent.0),
            goals_for,
            goals_against
        )),
        date: title.date,
        team,
        opponent,
        goals_for,
        goals_against,
        players: players_out,
        units,
        passes,
        goals,
        advantages,
        summary: team_stats::summary(&report.team_stats.entries[ours]),
        opponent_summary: team_stats::summary(&report.team_stats.entries[1 - ours]),
        team_stats: team_stat_rows(report),
        shot_zones_against: shot_zones_against(&report.opponent_shots),
        length,
        warnings,
    })
}

fn slug(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn resolve_units(
    report: &MatchReport,
    resolver: &Resolver<'_>,
    warnings: &mut Vec<String>,
) -> Vec<Unit> {
    report
        .ours
        .units
        .iter()
        .filter_map(|raw| {
            let players: Option<Vec<PlayerId>> =
                raw.members.iter().map(|m| resolver.by_jersey(m)).collect();
            if players.is_none() {
                warnings.push(format!(
                    "{:?} unit skipped: could not identify {}",
                    raw.kind,
                    raw.members
                        .iter()
                        .map(|m| m.surname.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Some(Unit {
                kind: raw.kind,
                players: players?,
                toi: raw.toi,
                stats: raw.stats.clone(),
            })
        })
        .collect()
}

fn resolve_shifts(
    timeline: Option<&Timeline>,
    resolver: &Resolver<'_>,
    warnings: &mut Vec<String>,
) -> HashMap<PlayerId, Vec<Interval>> {
    let Some(timeline) = timeline else {
        return HashMap::new();
    };
    let mut result = HashMap::new();
    for row in &timeline.rows {
        let TimelineRow { label, shifts, .. } = row;
        let toi = shifts.iter().map(|s| s.duration().0).sum::<f64>();
        match resolver.by_table_label(label, Some(toi)) {
            Some(id) => {
                result.insert(id, shifts.clone());
            }
            None => warnings.push(format!(
                "shift row {} not matched to a player",
                label.surname
            )),
        }
    }
    result
}

fn pass_matrix(
    raw: &crate::parse::matrix::RawMatrix,
    resolver: &Resolver<'_>,
    warnings: &mut Vec<String>,
) -> Option<PlayerMatrix> {
    let table_labels: Vec<&RowLabel> = resolver.table_rows.iter().map(|(_, l, _)| l).collect();
    let row_labels: Vec<&RowLabel> = raw.rows.iter().map(|(l, _)| l).collect();
    let column_labels: Vec<&RowLabel> = raw.columns.iter().collect();
    if row_labels != table_labels || column_labels != table_labels {
        warnings
            .push("passes page rows do not follow the player table order; passes skipped".into());
        return None;
    }
    let values = raw
        .rows
        .iter()
        .map(|(_, cells)| {
            cells
                .iter()
                .map(|c| c.count().unwrap_or_default())
                .collect()
        })
        .collect();
    Some(PlayerMatrix {
        players: resolver
            .table_rows
            .iter()
            .map(|(_, _, id)| id.clone())
            .collect(),
        values,
    })
}

/// Latest shift end just before the marker, else the marker minus its usual lag.
fn goal_time(marker: &GoalMarker, shifts: &HashMap<PlayerId, Vec<Interval>>) -> Seconds {
    let m = marker.marker_time.0;
    shifts
        .values()
        .flatten()
        .map(|s| s.end.0)
        .filter(|end| *end >= m - GOAL_SNAP_BEFORE && *end <= m + GOAL_SNAP_AFTER)
        .fold(None, |best: Option<f64>, end| {
            Some(best.map_or(end, |b| b.max(end)))
        })
        .map_or(Seconds(m - GOAL_MARKER_LAG), Seconds)
}

fn resolve_goals(
    timeline: &Timeline,
    ours: usize,
    advantages: &[Advantage],
    shifts: &HashMap<PlayerId, Vec<Interval>>,
) -> Vec<Goal> {
    let mut markers = timeline.goals.clone();
    markers.sort_by(|a, b| a.marker_time.0.total_cmp(&b.marker_time.0));
    let mut previous = (0, 0);
    let mut goals = Vec::new();
    for marker in &markers {
        let (a, b) = marker.score_after;
        let score_after = if ours == 0 { (a, b) } else { (b, a) };
        let scored_by = if score_after.0 > previous.0 {
            Team::Us
        } else {
            Team::Them
        };
        previous = score_after;
        let time = goal_time(marker, shifts);
        let probe = time.0 - ON_ICE_PROBE;
        let mut on_ice: Vec<PlayerId> = shifts
            .iter()
            .filter(|(_, list)| list.iter().any(|s| s.start.0 < probe && probe <= s.end.0))
            .map(|(id, _)| id.clone())
            .collect();
        on_ice.sort();
        let strength = advantages
            .iter()
            .find(|a| a.interval.start.0 - 1.0 < time.0 && time.0 <= a.interval.end.0 + 1.5)
            .map_or(Strength::Even, |a| match a.team {
                Team::Us => Strength::PowerPlay,
                Team::Them => Strength::ShortHanded,
            });
        goals.push(Goal {
            time,
            scored_by,
            strength,
            score_after,
            on_ice,
        });
    }
    goals
}

/// +/- counts even-strength and short-handed goals for, and goals against unless we were
/// short-handed. A mismatch with InStat's own column flags a reconstruction problem.
#[must_use]
pub fn plus_minus_from_goals(goals: &[Goal], id: &PlayerId) -> i64 {
    goals
        .iter()
        .filter(|g| g.on_ice.contains(id))
        .map(|g| match (g.scored_by, g.strength) {
            (Team::Us, Strength::PowerPlay) | (Team::Them, Strength::ShortHanded) => 0,
            (Team::Us, _) => 1,
            (Team::Them, _) => -1,
        })
        .sum()
}

fn check_plus_minus(players: &[Player], goals: &[Goal], warnings: &mut Vec<String>) {
    if goals.is_empty() {
        return;
    }
    for player in players {
        let (Some(skater), false) = (&player.skater, player.shifts.is_empty()) else {
            continue;
        };
        let rebuilt = plus_minus_from_goals(goals, &player.id);
        if rebuilt != skater.plus_minus {
            warnings.push(format!(
                "{}: +/- from shifts is {rebuilt}, InStat says {}",
                player.name, skater.plus_minus
            ));
        }
    }
}
