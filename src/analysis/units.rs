//! Lines and pairs as InStat reports them (defence pairs, forward lines, five-man units,
//! power-play and penalty-kill units), aggregated over games with shrinkage.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{BetaPrior, PlayerRef, Shrunk, fit_beta_prior, per_60, share_pct, shrink};
use crate::model::{GameId, PlayerId, Seconds, UnitKind, UnitStats};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitRow {
    pub kind: UnitKind,
    pub players: Vec<PlayerRef>,
    pub games: u32,
    pub toi: Seconds,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub penalties_drawn: u32,
    pub penalties_taken: u32,
    /// Time-weighted mean possession share.
    pub possession_pct: Option<f64>,
    pub corsi_pct: Option<f64>,
    pub corsi_for_60: Option<f64>,
    pub corsi_against_60: Option<f64>,
    pub goals_for_60: Option<f64>,
    pub goals_against_60: Option<f64>,
    pub shrunk_corsi: Option<Shrunk>,
    /// Special teams: shots for (PP) or against (PK) per 60, and time in the offensive zone.
    pub shots_60: Option<f64>,
    pub offensive_zone_share: Option<f64>,
    pub small_sample: bool,
    /// Per-game observations, for tests and charts.
    pub per_game: Vec<UnitGame>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitGame {
    pub game: GameId,
    pub toi: Seconds,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub goals_for: u32,
    pub goals_against: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitsReport {
    pub defence_pairs: Vec<UnitRow>,
    pub forward_lines: Vec<UnitRow>,
    pub full_units: Vec<UnitRow>,
    pub power_play: Vec<UnitRow>,
    pub penalty_kill: Vec<UnitRow>,
    /// Shrinkage priors per unit kind (CF share and its strength in pseudo-attempts).
    pub priors: Vec<(UnitKind, BetaPrior)>,
}

/// Units shorter than this in total are flagged as small samples.
const SMALL_SAMPLE_SECONDS: f64 = 10.0 * 60.0;

#[derive(Default)]
struct Accumulator {
    games: BTreeSet<GameId>,
    toi: f64,
    corsi_for: u32,
    corsi_against: u32,
    goals_for: u32,
    goals_against: u32,
    penalties_drawn: u32,
    penalties_taken: u32,
    possession_weighted: f64,
    possession_time: f64,
    shots: u32,
    offensive_zone: f64,
    per_game: Vec<UnitGame>,
}

type UnitKey = (UnitKind, Vec<PlayerId>);

fn accumulate(context: &Context<'_>) -> BTreeMap<UnitKey, Accumulator> {
    let mut map: BTreeMap<UnitKey, Accumulator> = BTreeMap::new();
    for game in &context.scope {
        for unit in &game.units {
            let mut members = unit.players.clone();
            members.sort();
            let acc = map.entry((unit.kind, members)).or_default();
            acc.games.insert(game.id.clone());
            acc.toi += unit.toi.0;
            let mut observation = UnitGame {
                game: game.id.clone(),
                toi: unit.toi,
                corsi_for: 0,
                corsi_against: 0,
                goals_for: 0,
                goals_against: 0,
            };
            match &unit.stats {
                UnitStats::EvenStrength(s) => {
                    acc.corsi_for += s.corsi_for;
                    acc.corsi_against += s.corsi_against;
                    acc.goals_for += s.goals_for;
                    acc.goals_against += s.goals_against;
                    acc.penalties_drawn += s.penalties_drawn;
                    acc.penalties_taken += s.penalties_taken;
                    if let Some(p) = s.possession_pct {
                        acc.possession_weighted += p * unit.toi.0;
                        acc.possession_time += unit.toi.0;
                    }
                    observation.corsi_for = s.corsi_for;
                    observation.corsi_against = s.corsi_against;
                    observation.goals_for = s.goals_for;
                    observation.goals_against = s.goals_against;
                }
                UnitStats::SpecialTeams(s) => {
                    let (for_goals, against_goals) = if unit.kind == UnitKind::PowerPlay {
                        (s.goals, 0)
                    } else {
                        (0, s.goals)
                    };
                    acc.goals_for += for_goals;
                    acc.goals_against += against_goals;
                    acc.shots += s.shots;
                    acc.offensive_zone += s.time_in_offensive_zone.0;
                    observation.goals_for = for_goals;
                    observation.goals_against = against_goals;
                }
            }
            acc.per_game.push(observation);
        }
    }
    map
}

fn row(context: &Context<'_>, kind: UnitKind, members: &[PlayerId], acc: Accumulator, prior: Option<BetaPrior>) -> Option<UnitRow> {
    let players: Vec<PlayerRef> = members
        .iter()
        .map(|id| context.roster.get(id).cloned())
        .collect::<Option<_>>()?;
    let toi = Seconds(acc.toi);
    let (cf, ca) = (f64::from(acc.corsi_for), f64::from(acc.corsi_against));
    let even_strength = matches!(kind, UnitKind::DefencePair | UnitKind::ForwardLine | UnitKind::FullUnit);
    let games = u32::try_from(acc.games.len()).unwrap_or(u32::MAX);
    Some(UnitRow {
        kind,
        players,
        games,
        toi,
        corsi_for: acc.corsi_for,
        corsi_against: acc.corsi_against,
        goals_for: acc.goals_for,
        goals_against: acc.goals_against,
        penalties_drawn: acc.penalties_drawn,
        penalties_taken: acc.penalties_taken,
        possession_pct: (acc.possession_time > 0.0).then(|| acc.possession_weighted / acc.possession_time),
        corsi_pct: if even_strength { share_pct(cf, ca) } else { None },
        corsi_for_60: if even_strength { per_60(cf, toi) } else { None },
        corsi_against_60: if even_strength { per_60(ca, toi) } else { None },
        goals_for_60: per_60(f64::from(acc.goals_for), toi),
        goals_against_60: per_60(f64::from(acc.goals_against), toi),
        shrunk_corsi: if even_strength { prior.and_then(|p| shrink(cf, ca, p)) } else { None },
        shots_60: if even_strength { None } else { per_60(f64::from(acc.shots), toi) },
        offensive_zone_share: (!even_strength && acc.toi > 0.0).then(|| 100.0 * acc.offensive_zone / acc.toi),
        small_sample: acc.toi < SMALL_SAMPLE_SECONDS,
        per_game: acc.per_game,
    })
}

#[must_use]
pub fn units(context: &Context<'_>) -> UnitsReport {
    let accumulated = accumulate(context);
    let priors: HashMap<UnitKind, Option<BetaPrior>> = [UnitKind::DefencePair, UnitKind::ForwardLine, UnitKind::FullUnit]
        .into_iter()
        .map(|kind| {
            let observations: Vec<(f64, f64)> = accumulated
                .iter()
                // Every unit informs the prior; method-of-moments already discounts small samples.
                .filter(|((k, _), acc)| *k == kind && acc.corsi_for + acc.corsi_against > 0)
                .map(|(_, acc)| (f64::from(acc.corsi_for), f64::from(acc.corsi_for + acc.corsi_against)))
                .collect();
            (kind, fit_beta_prior(&observations))
        })
        .collect();
    let mut report = UnitsReport {
        defence_pairs: Vec::new(),
        forward_lines: Vec::new(),
        full_units: Vec::new(),
        power_play: Vec::new(),
        penalty_kill: Vec::new(),
        priors: {
            let mut list: Vec<(UnitKind, BetaPrior)> = priors.iter().filter_map(|(k, p)| Some((*k, (*p)?))).collect();
            list.sort_by_key(|(k, _)| *k);
            list
        },
    };
    for ((kind, members), acc) in accumulated {
        let prior = priors.get(&kind).copied().flatten();
        let Some(unit_row) = row(context, kind, &members, acc, prior) else {
            continue;
        };
        let list = match kind {
            UnitKind::DefencePair => &mut report.defence_pairs,
            UnitKind::ForwardLine => &mut report.forward_lines,
            UnitKind::FullUnit => &mut report.full_units,
            UnitKind::PowerPlay => &mut report.power_play,
            UnitKind::PenaltyKill => &mut report.penalty_kill,
        };
        list.push(unit_row);
    }
    for list in [
        &mut report.defence_pairs,
        &mut report.forward_lines,
        &mut report.full_units,
        &mut report.power_play,
        &mut report.penalty_kill,
    ] {
        list.sort_by(|a, b| {
            b.toi.0.total_cmp(&a.toi.0).then_with(|| {
                let names = |u: &UnitRow| u.players.iter().map(|p| p.id.0.clone()).collect::<Vec<_>>();
                names(a).cmp(&names(b))
            })
        });
    }
    report
}
