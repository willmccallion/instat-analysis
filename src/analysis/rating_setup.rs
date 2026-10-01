//! What goes into a player rating: the stats a coach can choose from, how much each one
//! and each category counts, and ready-made presets.
//!
//! A rating is the weighted mean of its category scores; a category score is the weighted
//! mean of its stats' scores.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::analysis::common::{per_60, share_pct};
use crate::analysis::rankings::{RatingInput, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Offence,
    Defence,
    PuckPlay,
}

impl Category {
    pub const ALL: [Self; 3] = [Self::Offence, Self::Defence, Self::PuckPlay];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RatingStat {
    PointsPer60,
    GoalsPer60,
    ShotsPer60,
    XgPer60,
    EntriesPer60,
    AttemptsForPer60,
    TheirZoneRecoveriesPer60,
    ShotShareVsTeam,
    AttemptsAgainstPer60,
    XgAgainstPer60,
    BlocksPer60,
    HitsPer60,
    OwnZoneLossesPer60,
    BattlesWonPct,
    RecoveriesPer60,
    PuckLossesPer60,
    PassesPer60,
    FaceoffPct,
    CarryInPct,
}

fn f(v: u32) -> f64 {
    f64::from(v)
}

fn corsi_rel(i: &RatingInput) -> Option<f64> {
    let t = &i.totals;
    let on = share_pct(f(t.corsi_for), f(t.corsi_against))?;
    let off_for = (i.team_even_strength.0 - f(t.corsi_for)).max(0.0);
    let off_against = (i.team_even_strength.1 - f(t.corsi_against)).max(0.0);
    Some(on - share_pct(off_for, off_against)?)
}

fn percent(part: u32, whole: u32) -> Option<f64> {
    (whole > 0).then(|| 100.0 * f(part) / f(whole))
}

impl RatingStat {
    pub const ALL: [Self; 19] = [
        Self::PointsPer60,
        Self::GoalsPer60,
        Self::ShotsPer60,
        Self::XgPer60,
        Self::EntriesPer60,
        Self::AttemptsForPer60,
        Self::TheirZoneRecoveriesPer60,
        Self::ShotShareVsTeam,
        Self::AttemptsAgainstPer60,
        Self::XgAgainstPer60,
        Self::BlocksPer60,
        Self::HitsPer60,
        Self::OwnZoneLossesPer60,
        Self::BattlesWonPct,
        Self::RecoveriesPer60,
        Self::PuckLossesPer60,
        Self::PassesPer60,
        Self::FaceoffPct,
        Self::CarryInPct,
    ];

    /// Display name; also the key the UI's glossary explains.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PointsPer60 => "Points/60",
            Self::GoalsPer60 => "Goals/60",
            Self::ShotsPer60 => "Shots/60",
            Self::XgPer60 => "xG/60",
            Self::EntriesPer60 => "Entries/60",
            Self::AttemptsForPer60 => "Attempts for on ice/60",
            Self::TheirZoneRecoveriesPer60 => "Recoveries in their zone/60",
            Self::ShotShareVsTeam => "Shot share vs team",
            Self::AttemptsAgainstPer60 => "Attempts against/60",
            Self::XgAgainstPer60 => "xG against on ice/60",
            Self::BlocksPer60 => "Blocks/60",
            Self::HitsPer60 => "Hits/60",
            Self::OwnZoneLossesPer60 => "Own-zone losses/60",
            Self::BattlesWonPct => "Battles won %",
            Self::RecoveriesPer60 => "Recoveries/60",
            Self::PuckLossesPer60 => "Puck losses/60",
            Self::PassesPer60 => "Passes/60",
            Self::FaceoffPct => "Faceoff %",
            Self::CarryInPct => "Carry-in %",
        }
    }

    #[must_use]
    pub const fn category(self) -> Category {
        match self {
            Self::PointsPer60
            | Self::GoalsPer60
            | Self::ShotsPer60
            | Self::XgPer60
            | Self::EntriesPer60
            | Self::AttemptsForPer60
            | Self::TheirZoneRecoveriesPer60 => Category::Offence,
            Self::ShotShareVsTeam
            | Self::AttemptsAgainstPer60
            | Self::XgAgainstPer60
            | Self::BlocksPer60
            | Self::HitsPer60
            | Self::OwnZoneLossesPer60 => Category::Defence,
            Self::BattlesWonPct
            | Self::RecoveriesPer60
            | Self::PuckLossesPer60
            | Self::PassesPer60
            | Self::FaceoffPct
            | Self::CarryInPct => Category::PuckPlay,
        }
    }

    #[must_use]
    pub const fn higher_is_better(self) -> bool {
        !matches!(
            self,
            Self::AttemptsAgainstPer60 | Self::XgAgainstPer60 | Self::OwnZoneLossesPer60 | Self::PuckLossesPer60
        )
    }

    /// Only our Player report has these; opponents' zeros would mean "unknown", not "none".
    pub(crate) const fn needs_players_report(self) -> bool {
        matches!(self, Self::XgPer60 | Self::XgAgainstPer60 | Self::PassesPer60)
    }

    /// `None` when the player has nothing to measure (e.g. no faceoffs taken).
    pub(crate) fn value(self, i: &RatingInput) -> Option<f64> {
        if i.side == Side::Opponent && self.needs_players_report() {
            return None;
        }
        let t = &i.totals;
        match self {
            Self::PointsPer60 => per_60(f(t.points), t.toi),
            Self::GoalsPer60 => per_60(f(t.goals), t.toi),
            Self::ShotsPer60 => per_60(f(t.shots), t.toi),
            Self::XgPer60 => per_60(t.xg, t.toi),
            Self::EntriesPer60 => per_60(f(t.entries), t.toi),
            Self::AttemptsForPer60 => per_60(f(t.corsi_for), t.ev_toi),
            Self::TheirZoneRecoveriesPer60 => per_60(f(t.puck_recoveries_offensive_zone), t.toi),
            Self::ShotShareVsTeam => corsi_rel(i),
            Self::AttemptsAgainstPer60 => per_60(f(t.corsi_against), t.ev_toi),
            Self::XgAgainstPer60 => per_60(t.on_ice_xg_against, t.toi),
            Self::BlocksPer60 => per_60(f(t.blocked_shots), t.toi),
            Self::HitsPer60 => per_60(f(t.hits), t.toi),
            Self::OwnZoneLossesPer60 => per_60(f(t.puck_losses_defensive_zone), t.toi),
            Self::BattlesWonPct => percent(t.puck_battles_won, t.puck_battles),
            Self::RecoveriesPer60 => per_60(f(t.puck_recoveries), t.toi),
            Self::PuckLossesPer60 => per_60(f(t.puck_losses), t.toi),
            Self::PassesPer60 => per_60(f(t.passes), t.toi),
            Self::FaceoffPct => percent(t.faceoffs_won, t.faceoffs),
            Self::CarryInPct => percent(t.entry_types.carry, t.entries),
        }
    }
}

/// How much something counts: 0 (left out) to 3.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Weight(f64);

impl Weight {
    pub const MAX: f64 = 3.0;
    const ONE: Self = Self(1.0);

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Weight {
    type Error = String;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if (0.0..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(format!("weight {value} is outside 0–{}", Self::MAX))
        }
    }
}

impl From<Weight> for f64 {
    fn from(weight: Weight) -> Self {
        weight.0
    }
}

/// Weights for one position; stats missing from `stats` are left out of the rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionWeights {
    pub categories: BTreeMap<Category, Weight>,
    pub stats: BTreeMap<RatingStat, Weight>,
}

impl PositionWeights {
    fn from_lists(categories: &[(Category, f64)], stats: &[(RatingStat, f64)]) -> Self {
        Self {
            categories: categories.iter().map(|&(c, w)| (c, Weight(w))).collect(),
            stats: stats.iter().map(|&(s, w)| (s, Weight(w))).collect(),
        }
    }

    #[must_use]
    pub fn category(&self, category: Category) -> f64 {
        self.categories.get(&category).map_or(0.0, |w| w.get())
    }

    #[must_use]
    pub fn stat(&self, stat: RatingStat) -> f64 {
        self.stats.get(&stat).map_or(0.0, |w| w.get())
    }

    /// The same stats, each counting once, with the three categories equal.
    fn flattened(&self) -> Self {
        Self {
            categories: Category::ALL.into_iter().map(|c| (c, Weight::ONE)).collect(),
            stats: self.stats.keys().map(|&s| (s, Weight::ONE)).collect(),
        }
    }

    /// `category` counts most (3) and the other two least (0.5); stat weights are unchanged.
    fn focus_on(mut self, category: Category) -> Self {
        for c in Category::ALL {
            let weight = if c == category { Weight::MAX } else { 0.5 };
            self.categories.insert(c, Weight(weight));
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingWeights {
    pub forwards: PositionWeights,
    pub defence: PositionWeights,
}

use Category::{Defence as D, Offence as O, PuckPlay as P};
use RatingStat as S;

/// Forwards are judged a little more on creating offence; every stat weight reflects how
/// directly it shows up in goals for and against.
const FORWARD_CATEGORIES: [(Category, f64); 3] = [(O, 1.5), (D, 1.0), (P, 1.0)];
const FORWARD_STATS: [(RatingStat, f64); 18] = [
    (S::PointsPer60, 3.0),
    (S::XgPer60, 2.5),
    (S::GoalsPer60, 1.5),
    (S::ShotsPer60, 1.5),
    (S::AttemptsForPer60, 1.5),
    (S::EntriesPer60, 1.0),
    (S::TheirZoneRecoveriesPer60, 1.0),
    (S::ShotShareVsTeam, 3.0),
    (S::XgAgainstPer60, 2.0),
    (S::AttemptsAgainstPer60, 1.5),
    (S::OwnZoneLossesPer60, 1.5),
    (S::BlocksPer60, 0.5),
    (S::BattlesWonPct, 2.0),
    (S::RecoveriesPer60, 1.5),
    (S::PuckLossesPer60, 1.5),
    (S::PassesPer60, 1.0),
    (S::FaceoffPct, 1.0),
    (S::CarryInPct, 1.0),
];

/// Defencemen are judged a little more on keeping the puck out of our net.
const DEFENCE_CATEGORIES: [(Category, f64); 3] = [(O, 1.0), (D, 1.5), (P, 1.0)];
const DEFENCE_STATS: [(RatingStat, f64); 18] = [
    (S::PointsPer60, 2.0),
    (S::AttemptsForPer60, 2.0),
    (S::ShotsPer60, 1.0),
    (S::XgPer60, 1.0),
    (S::GoalsPer60, 0.5),
    (S::EntriesPer60, 0.5),
    (S::TheirZoneRecoveriesPer60, 0.5),
    (S::ShotShareVsTeam, 3.0),
    (S::AttemptsAgainstPer60, 2.5),
    (S::XgAgainstPer60, 2.5),
    (S::OwnZoneLossesPer60, 2.0),
    (S::BlocksPer60, 1.0),
    (S::HitsPer60, 0.5),
    (S::BattlesWonPct, 2.0),
    (S::RecoveriesPer60, 2.0),
    (S::PuckLossesPer60, 1.5),
    (S::PassesPer60, 1.5),
    (S::CarryInPct, 0.5),
];

impl Default for RatingWeights {
    fn default() -> Self {
        Self {
            forwards: PositionWeights::from_lists(&FORWARD_CATEGORIES, &FORWARD_STATS),
            defence: PositionWeights::from_lists(&DEFENCE_CATEGORIES, &DEFENCE_STATS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    pub weights: RatingWeights,
}

/// Ready-made starting points; the first is the default.
#[must_use]
pub fn presets() -> Vec<Preset> {
    let base = RatingWeights::default();
    let both = |change: &dyn Fn(&PositionWeights) -> PositionWeights| RatingWeights {
        forwards: change(&base.forwards),
        defence: change(&base.defence),
    };
    vec![
        Preset {
            name: "Recommended",
            description: "Every stat weighted by how directly it leads to goals for and against; forwards lean to offence, defence to defence.",
            weights: base.clone(),
        },
        Preset {
            name: "Offence first",
            description: "Offence counts most (3) and the other two parts least (0.5); the stats inside each part keep their recommended weights.",
            weights: both(&|w| w.clone().focus_on(O)),
        },
        Preset {
            name: "Defence first",
            description: "Defence counts most (3) and the other two parts least (0.5); the stats inside each part keep their recommended weights.",
            weights: both(&|w| w.clone().focus_on(D)),
        },
        Preset {
            name: "Puck control",
            description: "Puck play (battles, recoveries, giveaways, passing) counts most (3) and the other two parts least (0.5).",
            weights: both(&|w| w.clone().focus_on(P)),
        },
        Preset {
            name: "Simple balanced",
            description: "The same stats, each counting once, with offence, defence and puck play equal.",
            weights: both(&PositionWeights::flattened),
        },
    ]
}

/// A stat as the settings screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatInfo {
    pub stat: RatingStat,
    pub name: &'static str,
    pub category: Category,
    pub higher_is_better: bool,
}

#[must_use]
pub fn catalogue() -> Vec<StatInfo> {
    RatingStat::ALL
        .into_iter()
        .map(|stat| StatInfo {
            stat,
            name: stat.name(),
            category: stat.category(),
            higher_is_better: stat.higher_is_better(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{RatingWeights, Weight, presets};

    #[test]
    fn weights_outside_zero_to_three_are_rejected() {
        assert!(serde_json::from_str::<Weight>("2.5").is_ok());
        assert!(serde_json::from_str::<Weight>("-1").is_err());
        assert!(serde_json::from_str::<Weight>("4").is_err());
    }

    #[test]
    fn default_weights_survive_a_round_trip_through_json() {
        let json = serde_json::to_string(&RatingWeights::default()).unwrap();

        assert_eq!(serde_json::from_str::<RatingWeights>(&json).unwrap(), RatingWeights::default());
    }

    #[test]
    fn the_first_preset_is_the_default() {
        assert_eq!(presets()[0].weights, RatingWeights::default());
    }

    #[test]
    fn default_weights_are_within_range() {
        let weights = RatingWeights::default();
        for position in [&weights.forwards, &weights.defence] {
            for w in position.categories.values().chain(position.stats.values()) {
                assert!(Weight::try_from(w.get()).is_ok());
            }
        }
    }
}
