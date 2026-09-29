//! Every pair of skaters: time together (from shifts), goals with and without each other,
//! shot attempts together (from InStat's line tables) and passes between them.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{BetaPrior, PlayerRef, Shrunk, share_pct, shrink};
use crate::analysis::passing::PassTotals;
use crate::model::{GameId, PlayerId, Position, Seconds, Strength, UnitKind, UnitStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum PairKind {
    DefenceDefence,
    ForwardForward,
    ForwardDefence,
    Other,
}

impl PairKind {
    const fn of(a: Position, b: Position) -> Self {
        match (a, b) {
            (Position::Defence, Position::Defence) => Self::DefenceDefence,
            (Position::Forward, Position::Forward) => Self::ForwardForward,
            (Position::Forward, Position::Defence) | (Position::Defence, Position::Forward) => {
                Self::ForwardDefence
            }
            _ => Self::Other,
        }
    }

    /// Which InStat line table carries shot attempts for this kind of pair.
    const fn corsi_source(self) -> Option<UnitKind> {
        match self {
            Self::DefenceDefence => Some(UnitKind::DefencePair),
            Self::ForwardForward => Some(UnitKind::ForwardLine),
            Self::ForwardDefence => Some(UnitKind::FullUnit),
            Self::Other => None,
        }
    }
}

/// On-ice results with or without the partner, at even strength.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct OnIce {
    pub toi: Seconds,
    pub goals_for: u32,
    pub goals_against: u32,
    pub goals_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairCorsi {
    pub source: UnitKind,
    pub toi: Seconds,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub corsi_pct: Option<f64>,
    pub shrunk: Option<Shrunk>,
    /// Each player's CF% in the rest of their even-strength time.
    pub a_apart_pct: Option<f64>,
    pub b_apart_pct: Option<f64>,
    /// Share of their shift-measured time together that the line tables cover.
    pub coverage: Option<f64>,
    /// CF% expected from the two players' individual impact ratings (filled later).
    pub expected_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairGame {
    pub game: GameId,
    pub toi_together: Seconds,
    pub corsi_for: u32,
    pub corsi_against: u32,
    pub a_apart_for: u32,
    pub a_apart_against: u32,
    pub b_apart_for: u32,
    pub b_apart_against: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairRow {
    pub a: PlayerRef,
    pub b: PlayerRef,
    pub kind: PairKind,
    pub games_together: u32,
    pub together: OnIce,
    pub pp_toi: Seconds,
    pub sh_toi: Seconds,
    pub a_without_b: OnIce,
    pub b_without_a: OnIce,
    pub corsi: Option<PairCorsi>,
    pub passes_a_to_b: u32,
    pub passes_b_to_a: u32,
    /// Passes between them relative to what their overall passing volume predicts.
    pub pass_lift: Option<f64>,
    pub per_game: Vec<PairGame>,
}

#[derive(Default, Clone)]
struct IceTotals {
    ev_toi: f64,
    gf: u32,
    ga: u32,
}

fn on_ice(t: &IceTotals) -> OnIce {
    OnIce {
        toi: Seconds(t.ev_toi),
        goals_for: t.gf,
        goals_against: t.ga,
        goals_pct: share_pct(f64::from(t.gf), f64::from(t.ga)),
    }
}

struct PairAccumulator {
    together: IceTotals,
    pp: f64,
    sh: f64,
    games: std::collections::BTreeSet<GameId>,
}

/// Per-player even-strength totals and per-pair together totals from stints.
fn shift_totals(context: &Context<'_>) -> (BTreeMap<PlayerId, IceTotals>, BTreeMap<(PlayerId, PlayerId), PairAccumulator>) {
    let mut players: BTreeMap<PlayerId, IceTotals> = BTreeMap::new();
    let mut pairs: BTreeMap<(PlayerId, PlayerId), PairAccumulator> = BTreeMap::new();
    for stint in &context.stints {
        let duration = stint.duration().0;
        for (i, a) in stint.players.iter().enumerate() {
            if stint.strength == Strength::Even {
                let entry = players.entry(a.clone()).or_default();
                entry.ev_toi += duration;
                entry.gf += stint.goals_for;
                entry.ga += stint.goals_against;
            }
            for b in &stint.players[i + 1..] {
                let pair = pairs.entry((a.clone(), b.clone())).or_insert_with(|| PairAccumulator {
                    together: IceTotals::default(),
                    pp: 0.0,
                    sh: 0.0,
                    games: std::collections::BTreeSet::new(),
                });
                match stint.strength {
                    Strength::Even => {
                        pair.together.ev_toi += duration;
                        pair.together.gf += stint.goals_for;
                        pair.together.ga += stint.goals_against;
                        pair.games.insert(stint.game.clone());
                    }
                    Strength::PowerPlay => pair.pp += duration,
                    Strength::ShortHanded => pair.sh += duration,
                }
            }
        }
    }
    (players, pairs)
}

fn apart(total: &IceTotals, together: &IceTotals) -> OnIce {
    on_ice(&IceTotals {
        ev_toi: (total.ev_toi - together.ev_toi).max(0.0),
        gf: total.gf.saturating_sub(together.gf),
        ga: total.ga.saturating_sub(together.ga),
    })
}

/// Shot attempts together for a pair in one game, from line tables of `source` kind.
fn unit_corsi(context: &Context<'_>, game: &GameId, source: UnitKind, a: &PlayerId, b: &PlayerId) -> (f64, u32, u32) {
    context
        .scope
        .iter()
        .filter(|g| &g.id == game)
        .flat_map(|g| g.units.iter())
        .filter(|u| u.kind == source && u.players.contains(a) && u.players.contains(b))
        .fold((0.0, 0, 0), |(toi, cf, ca), u| match &u.stats {
            UnitStats::EvenStrength(s) => (toi + u.toi.0, cf + s.corsi_for, ca + s.corsi_against),
            UnitStats::SpecialTeams(_) => (toi, cf, ca),
        })
}

fn player_corsi(context: &Context<'_>, game: &GameId, id: &PlayerId) -> (u32, u32) {
    context
        .scope
        .iter()
        .filter(|g| &g.id == game)
        .filter_map(|g| g.player(id)?.skater.as_ref())
        .fold((0, 0), |(f, a), s| (f + s.corsi_for, a + s.corsi_against))
}

fn pair_corsi(
    context: &Context<'_>,
    kind: PairKind,
    a: &PlayerId,
    b: &PlayerId,
    shift_toi_together: f64,
    priors: &[(UnitKind, BetaPrior)],
) -> (Option<PairCorsi>, Vec<PairGame>) {
    let Some(source) = kind.corsi_source() else {
        return (None, Vec::new());
    };
    let mut per_game = Vec::new();
    let (mut toi, mut cf, mut ca) = (0.0, 0, 0);
    let (mut a_total, mut b_total) = ((0, 0), (0, 0));
    for game in &context.scope {
        let (a_played, b_played) = (game.player(a).is_some(), game.player(b).is_some());
        if !(a_played && b_played) {
            continue;
        }
        let (together_toi, together_for, together_against) = unit_corsi(context, &game.id, source, a, b);
        let a_game = player_corsi(context, &game.id, a);
        let b_game = player_corsi(context, &game.id, b);
        toi += together_toi;
        cf += together_for;
        ca += together_against;
        a_total = (a_total.0 + a_game.0, a_total.1 + a_game.1);
        b_total = (b_total.0 + b_game.0, b_total.1 + b_game.1);
        per_game.push(PairGame {
            game: game.id.clone(),
            toi_together: Seconds(together_toi),
            corsi_for: together_for,
            corsi_against: together_against,
            a_apart_for: a_game.0.saturating_sub(together_for),
            a_apart_against: a_game.1.saturating_sub(together_against),
            b_apart_for: b_game.0.saturating_sub(together_for),
            b_apart_against: b_game.1.saturating_sub(together_against),
        });
    }
    if toi <= 0.0 {
        return (None, per_game);
    }
    let f = f64::from;
    let apart_pct = |total: (u32, u32)| share_pct(f(total.0.saturating_sub(cf)), f(total.1.saturating_sub(ca)));
    let corsi = PairCorsi {
        source,
        toi: Seconds(toi),
        corsi_for: cf,
        corsi_against: ca,
        corsi_pct: share_pct(f(cf), f(ca)),
        shrunk: priors
            .iter()
            .find(|(kind, _)| *kind == source)
            .and_then(|(_, p)| shrink(f(cf), f(ca), *p)),
        a_apart_pct: apart_pct(a_total),
        b_apart_pct: apart_pct(b_total),
        coverage: (shift_toi_together > 0.0).then(|| (toi / shift_toi_together).min(1.0)),
        expected_pct: None,
    };
    (Some(corsi), per_game)
}

#[must_use]
pub fn pairs(
    context: &Context<'_>,
    passes: &PassTotals,
    priors: &[(UnitKind, BetaPrior)],
) -> Vec<PairRow> {
    let (players, together) = shift_totals(context);
    let mut rows = Vec::new();
    for ((a, b), acc) in together {
        let (Some(a_ref), Some(b_ref)) = (context.roster.get(&a), context.roster.get(&b)) else {
            continue;
        };
        if acc.together.ev_toi <= 0.0 && acc.pp <= 0.0 && acc.sh <= 0.0 {
            continue;
        }
        let kind = PairKind::of(a_ref.position, b_ref.position);
        let empty = IceTotals::default();
        let a_total = players.get(&a).unwrap_or(&empty);
        let b_total = players.get(&b).unwrap_or(&empty);
        let (corsi, per_game) = pair_corsi(context, kind, &a, &b, acc.together.ev_toi, priors);
        rows.push(PairRow {
            a: a_ref.clone(),
            b: b_ref.clone(),
            kind,
            games_together: u32::try_from(acc.games.len()).unwrap_or(u32::MAX),
            together: on_ice(&acc.together),
            pp_toi: Seconds(acc.pp),
            sh_toi: Seconds(acc.sh),
            a_without_b: apart(a_total, &acc.together),
            b_without_a: apart(b_total, &acc.together),
            corsi,
            passes_a_to_b: passes.between(&a, &b),
            passes_b_to_a: passes.between(&b, &a),
            pass_lift: passes.lift(&a, &b),
            per_game,
        });
    }
    rows.sort_by(|x, y| {
        y.together
            .toi
            .0
            .total_cmp(&x.together.toi.0)
            .then_with(|| (&x.a.id, &x.b.id).cmp(&(&y.a.id, &y.b.id)))
    });
    rows
}
