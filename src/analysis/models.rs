//! Season-level models: what goes with winning (Firth logistic + OLS) and line results
//! adjusted for opponent strength (Poisson random-intercept model).

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{PlayerRef, TestRow, Verdict, share_pct};
use crate::analysis::impact::{Observation, unit_observations};
use crate::analysis::team::{Outcome, outcome};
use crate::model::{Game, PlayerId, UnitKind};
use crate::stats::glm::{fit_firth_logistic, fit_ols, fit_poisson_random_intercept};

pub const MIN_GAMES_WIN_MODEL: usize = 10;
pub const MIN_GAMES_OPPONENT_MODEL: usize = 5;
pub const MIN_OPPONENTS: usize = 3;
const MIN_GAMES_MULTIVARIABLE: usize = 15;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Predictor {
    pub name: String,
    pub unit: String,
    pub games: usize,
    /// Odds ratio for a win per `unit` increase, with 95% Wald interval.
    pub odds_ratio: Option<f64>,
    pub odds_ratio_ci: Option<(f64, f64)>,
    pub odds_p: Option<f64>,
    /// Goal differential per `unit` increase (OLS).
    pub goal_diff_slope: Option<f64>,
    pub goal_diff_p: Option<f64>,
    pub r_squared: Option<f64>,
    /// (predictor value, goal differential, won) per game, for scatter plots.
    pub points: Vec<(f64, f64, bool)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Multivariable {
    pub predictors: Vec<String>,
    pub coefficients: Vec<(String, f64, f64, f64)>,
    pub r_squared: f64,
    pub adjusted_r_squared: f64,
    pub f_statistic: f64,
    pub f_p: f64,
    pub loo_accuracy: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdjustedUnit {
    pub kind: UnitKind,
    pub players: Vec<PlayerRef>,
    pub raw_corsi_pct: Option<f64>,
    pub adjusted_corsi_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpponentEffect {
    pub opponent: String,
    /// Multiplier on shot attempts against us (> 1 = tougher opponent).
    pub attempts_for_them: f64,
    pub attempts_for_us: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelsReport {
    pub games: usize,
    pub win_model_ready: bool,
    pub win_model_needs: usize,
    pub predictors: Vec<Predictor>,
    pub multivariable: Option<Multivariable>,
    pub opponent_model_ready: bool,
    pub opponent_model_needs: String,
    pub adjusted_units: Vec<AdjustedUnit>,
    pub opponents: Vec<OpponentEffect>,
}

type Extractor = fn(&Game) -> Option<f64>;

const PREDICTORS: [(&str, &str, f64, Extractor); 6] = [
    ("Even-strength shot share (CF%)", "5 percentage points", 5.0, |g| {
        share_pct(f64::from(g.summary.even_strength_shots.0), f64::from(g.opponent_summary.even_strength_shots.0))
    }),
    ("Expected-goals share (xG%)", "5 percentage points", 5.0, |g| {
        g.summary.xg.zip(g.opponent_summary.xg).and_then(|(a, b)| share_pct(a, b))
    }),
    ("Shots on goal share", "5 percentage points", 5.0, |g| {
        share_pct(f64::from(g.summary.shots_on_goal), f64::from(g.opponent_summary.shots_on_goal))
    }),
    ("Power plays minus times short-handed", "1 extra power play", 1.0, |g| {
        Some(f64::from(g.summary.power_plays) - f64::from(g.opponent_summary.power_plays))
    }),
    ("Faceoff win %", "5 percentage points", 5.0, |g| {
        share_pct(f64::from(g.summary.faceoffs_won), f64::from(g.opponent_summary.faceoffs_won))
    }),
    ("Puck possession %", "5 percentage points", 5.0, |g| g.summary.possession_pct),
];

fn predictor(games: &[&Game], name: &str, unit: &str, scale: f64, extract: Extractor) -> Predictor {
    let points: Vec<(f64, f64, bool)> = games
        .iter()
        .filter_map(|g| {
            let value = extract(g)?;
            Some((value, f64::from(g.goals_for) - f64::from(g.goals_against), outcome(g) == Outcome::Win))
        })
        .collect();
    let n = points.len();
    let x = DMatrix::from_fn(n, 2, |i, j| if j == 0 { 1.0 } else { points[i].0 / scale });
    let won = DVector::from_iterator(n, points.iter().map(|p| f64::from(u8::from(p.2))));
    let diff = DVector::from_iterator(n, points.iter().map(|p| p.1));
    let logistic = (n >= MIN_GAMES_WIN_MODEL).then(|| fit_firth_logistic(&x, &won)).flatten();
    let ols = (n >= MIN_GAMES_WIN_MODEL).then(|| fit_ols(&x, &diff)).flatten();
    Predictor {
        name: name.to_owned(),
        unit: unit.to_owned(),
        games: n,
        odds_ratio: logistic.as_ref().map(|f| f.coefficients[1].exp()),
        odds_ratio_ci: logistic
            .as_ref()
            .map(|f| ((f.coefficients[1] - 1.96 * f.standard_errors[1]).exp(), (f.coefficients[1] + 1.96 * f.standard_errors[1]).exp())),
        odds_p: logistic.as_ref().map(|f| f.p_values[1]),
        goal_diff_slope: ols.as_ref().map(|f| f.coefficients[1]),
        goal_diff_p: ols.as_ref().map(|f| f.p_values[1]),
        r_squared: ols.as_ref().map(|f| f.r_squared),
        points,
    }
}

fn multivariable(games: &[&Game]) -> Option<Multivariable> {
    if games.len() < MIN_GAMES_MULTIVARIABLE {
        return None;
    }
    let chosen: Vec<&(&str, &str, f64, Extractor)> = [0, 1, 3].iter().map(|&i| &PREDICTORS[i]).collect();
    let rows: Vec<(Vec<f64>, f64, bool)> = games
        .iter()
        .filter_map(|g| {
            let values: Option<Vec<f64>> = chosen.iter().map(|(_, _, scale, f)| f(g).map(|v| v / scale)).collect();
            Some((values?, f64::from(g.goals_for) - f64::from(g.goals_against), outcome(g) == Outcome::Win))
        })
        .collect();
    let n = rows.len();
    if n < MIN_GAMES_MULTIVARIABLE {
        return None;
    }
    let p = chosen.len() + 1;
    let x = DMatrix::from_fn(n, p, |i, j| if j == 0 { 1.0 } else { rows[i].0[j - 1] });
    let y = DVector::from_iterator(n, rows.iter().map(|r| r.1));
    let fit = fit_ols(&x, &y)?;
    let won = DVector::from_iterator(n, rows.iter().map(|r| f64::from(u8::from(r.2))));
    let correct = (0..n)
        .filter(|&held_out| {
            let keep: Vec<usize> = (0..n).filter(|&i| i != held_out).collect();
            let train_x = DMatrix::from_fn(keep.len(), p, |i, j| x[(keep[i], j)]);
            let train_y = DVector::from_fn(keep.len(), |i, _| won[keep[i]]);
            fit_firth_logistic(&train_x, &train_y).is_some_and(|f| {
                let eta: f64 = (0..p).map(|j| x[(held_out, j)] * f.coefficients[j]).sum();
                (eta > 0.0) == (won[held_out] > 0.5)
            })
        })
        .count();
    Some(Multivariable {
        predictors: chosen.iter().map(|c| c.0.to_owned()).collect(),
        coefficients: std::iter::once("Intercept".to_owned())
            .chain(chosen.iter().map(|c| c.0.to_owned()))
            .enumerate()
            .map(|(j, name)| (name, fit.coefficients[j], fit.standard_errors[j], fit.p_values[j]))
            .collect(),
        r_squared: fit.r_squared,
        adjusted_r_squared: fit.adjusted_r_squared,
        f_statistic: fit.f_statistic,
        f_p: fit.f_p_value,
        loo_accuracy: Some(correct as f64 / n as f64),
    })
}

fn opponent_model(context: &Context<'_>) -> (Vec<AdjustedUnit>, Vec<OpponentEffect>) {
    let mut opponents: Vec<String> = context.scope.iter().map(|g| g.opponent.0.clone()).collect();
    opponents.sort();
    opponents.dedup();
    let opponent_of = |o: &Observation| {
        context
            .scope
            .iter()
            .find(|g| g.id == o.game)
            .and_then(|g| opponents.iter().position(|name| name == &g.opponent.0))
    };
    let mut adjusted = Vec::new();
    let mut effects: Vec<(f64, f64, usize)> = vec![(0.0, 0.0, 0); opponents.len()];
    for kind in [UnitKind::DefencePair, UnitKind::ForwardLine] {
        let observations: Vec<Observation> = unit_observations(context, kind)
            .into_iter()
            .filter(|o| o.toi.0 > 0.0)
            .collect();
        let groups: Option<Vec<usize>> = observations.iter().map(opponent_of).collect();
        let Some(groups) = groups else {
            continue;
        };
        let mut units: Vec<Vec<PlayerId>> = observations
            .iter()
            .map(|o| {
                let mut p = o.players.clone();
                p.sort();
                p
            })
            .collect();
        units.sort();
        units.dedup();
        let n = observations.len();
        let x = DMatrix::from_fn(n, units.len(), |i, j| {
            let mut p = observations[i].players.clone();
            p.sort();
            f64::from(u8::from(p == units[j]))
        });
        let offset = DVector::from_iterator(n, observations.iter().map(|o| (o.toi.0 / 3600.0).ln()));
        let penalty = DVector::from_element(units.len(), 1e-3);
        let fit = |counts: Vec<f64>| {
            fit_poisson_random_intercept(&x, &DVector::from_vec(counts), &offset, &groups, opponents.len(), &penalty)
        };
        let (Some(for_fit), Some(against_fit)) = (
            fit(observations.iter().map(|o| o.count_for).collect()),
            fit(observations.iter().map(|o| o.count_against).collect()),
        ) else {
            continue;
        };
        for (j, members) in units.iter().enumerate() {
            let (rate_for, rate_against) = (for_fit.fixed.coefficients[j].exp(), against_fit.fixed.coefficients[j].exp());
            let players: Option<Vec<PlayerRef>> = members.iter().map(|id| context.roster.get(id).cloned()).collect();
            let (raw_for, raw_against) = observations
                .iter()
                .filter(|o| {
                    let mut p = o.players.clone();
                    p.sort();
                    &p == members
                })
                .fold((0.0, 0.0), |(f, a), o| (f + o.count_for, a + o.count_against));
            if let Some(players) = players {
                adjusted.push(AdjustedUnit {
                    kind,
                    players,
                    raw_corsi_pct: share_pct(raw_for, raw_against),
                    adjusted_corsi_pct: 100.0 * rate_for / (rate_for + rate_against),
                });
            }
        }
        for (g, effect) in effects.iter_mut().enumerate() {
            effect.0 += against_fit.random_effects[g];
            effect.1 += for_fit.random_effects[g];
            effect.2 += 1;
        }
    }
    let opponent_effects = opponents
        .iter()
        .zip(effects)
        .filter(|(_, e)| e.2 > 0)
        .map(|(name, (them, us, k))| OpponentEffect {
            opponent: name.clone(),
            attempts_for_them: (them / k as f64).exp(),
            attempts_for_us: (us / k as f64).exp(),
        })
        .collect();
    (adjusted, opponent_effects)
}

pub fn models(context: &Context<'_>, tests: &mut Vec<TestRow>) -> ModelsReport {
    let games = context.scope.len();
    let win_ready = games >= MIN_GAMES_WIN_MODEL;
    let predictors: Vec<Predictor> = if win_ready {
        PREDICTORS
            .iter()
            .map(|(name, unit, scale, f)| predictor(&context.scope, name, unit, *scale, *f))
            .collect()
    } else {
        Vec::new()
    };
    let family = "What wins games";
    if win_ready {
        for p in &predictors {
            tests.push(TestRow {
                family: family.to_owned(),
                question: format!("Do we win more when our {} is higher?", p.name.to_lowercase()),
                method: "Firth-penalised logistic regression (win vs not), Wald test".to_owned(),
                statistic_label: "Odds ratio".to_owned(),
                statistic: p.odds_ratio,
                df: None,
                p: p.odds_p,
                p_adjusted: None,
                effect_label: format!("Goal differential per {}", p.unit),
                effect: p.goal_diff_slope,
                ci: p.odds_ratio_ci,
                n: format!("{} games", p.games),
                secondary: p.goal_diff_p.map(|gp| format!("OLS on goal differential: p = {gp:.3}, R² = {:.2}", p.r_squared.unwrap_or(f64::NAN))),
                verdict: Verdict::from_adjusted_p(p.odds_p),
                plain: p.odds_ratio.map_or_else(
                    || "Could not fit.".to_owned(),
                    |or| format!("Each {} more multiplies the odds of winning by {or:.2}.", p.unit),
                ),
                assumptions: "Games independent; linear effect on the log-odds scale.".to_owned(),
            });
        }
    } else {
        tests.push(TestRow::not_enough(
            family,
            "Which team stats go with winning?",
            "Firth-penalised logistic regression and OLS on goal differential",
            format!("{games} of {MIN_GAMES_WIN_MODEL} games"),
            "the win model switches on at 10 games",
        ));
    }
    let mut opponents: Vec<&str> = context.scope.iter().map(|g| g.opponent.0.as_str()).collect();
    opponents.sort_unstable();
    opponents.dedup();
    let opponent_ready = games >= MIN_GAMES_OPPONENT_MODEL && opponents.len() >= MIN_OPPONENTS;
    let (adjusted_units, opponent_effects) = if opponent_ready { opponent_model(context) } else { (Vec::new(), Vec::new()) };
    ModelsReport {
        games,
        win_model_ready: win_ready,
        win_model_needs: MIN_GAMES_WIN_MODEL,
        multivariable: multivariable(&context.scope),
        predictors,
        opponent_model_ready: opponent_ready,
        opponent_model_needs: format!("{MIN_GAMES_OPPONENT_MODEL} games against {MIN_OPPONENTS}+ different opponents"),
        adjusted_units,
        opponents: opponent_effects,
    }
}
