//! Shared types and helpers for the analysis report.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::{Game, PlayerId, Position, Seconds};
use crate::stats::dist;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerRef {
    pub id: PlayerId,
    pub name: String,
    pub jersey: Option<u16>,
    pub position: Position,
}

/// Latest known name/jersey and most frequent position for every player in `games`.
#[must_use]
pub fn roster(games: &[&Game]) -> HashMap<PlayerId, PlayerRef> {
    let mut positions: HashMap<PlayerId, HashMap<Position, u32>> = HashMap::new();
    let mut result: HashMap<PlayerId, PlayerRef> = HashMap::new();
    for game in games {
        for player in &game.players {
            *positions
                .entry(player.id.clone())
                .or_default()
                .entry(player.position)
                .or_default() += 1;
            result.insert(
                player.id.clone(),
                PlayerRef {
                    id: player.id.clone(),
                    name: player.name.clone(),
                    jersey: player.jersey.map(|j| j.0),
                    position: player.position,
                },
            );
        }
    }
    for (id, counts) in positions {
        let known = counts
            .iter()
            .filter(|(p, _)| **p != Position::Unknown)
            .max_by_key(|(p, n)| (**n, tie_break(**p)))
            .map(|(p, _)| *p);
        if let (Some(position), Some(entry)) = (known, result.get_mut(&id)) {
            entry.position = position;
        }
    }
    result
}

/// Deterministic preference when a player's positions are equally frequent.
const fn tie_break(position: Position) -> u8 {
    match position {
        Position::Goalie => 3,
        Position::Defence => 2,
        Position::Forward => 1,
        Position::Unknown => 0,
    }
}

/// A point estimate with an interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Estimate {
    pub value: f64,
    pub low: f64,
    pub high: f64,
}

#[must_use]
pub fn per_60(count: f64, time: Seconds) -> Option<f64> {
    (time.0 > 0.0).then(|| count * 3600.0 / time.0)
}

/// `a / (a + b)` as a percentage.
#[must_use]
pub fn share_pct(a: f64, b: f64) -> Option<f64> {
    (a + b > 0.0).then(|| 100.0 * a / (a + b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Verdict {
    LikelyReal,
    Maybe,
    CouldBeNoise,
    NotEnoughData,
}

impl Verdict {
    /// From a multiple-testing-adjusted p-value.
    #[must_use]
    pub fn from_adjusted_p(p: Option<f64>) -> Self {
        match p {
            None => Self::NotEnoughData,
            Some(p) if p < 0.05 => Self::LikelyReal,
            Some(p) if p < 0.20 => Self::Maybe,
            Some(_) => Self::CouldBeNoise,
        }
    }
}

/// One row in the Advanced statistics tables; `plain` is the coach-facing sentence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestRow {
    pub family: String,
    pub question: String,
    pub method: String,
    pub statistic_label: String,
    pub statistic: Option<f64>,
    pub df: Option<String>,
    pub p: Option<f64>,
    pub p_adjusted: Option<f64>,
    pub effect_label: String,
    pub effect: Option<f64>,
    pub ci: Option<(f64, f64)>,
    pub n: String,
    /// A second, complementary result (e.g. a robustness check).
    pub secondary: Option<String>,
    pub verdict: Verdict,
    pub plain: String,
    pub assumptions: String,
}

impl TestRow {
    #[must_use]
    pub fn not_enough(family: &str, question: &str, method: &str, n: String, reason: &str) -> Self {
        Self {
            family: family.to_owned(),
            question: question.to_owned(),
            method: method.to_owned(),
            statistic_label: String::new(),
            statistic: None,
            df: None,
            p: None,
            p_adjusted: None,
            effect_label: String::new(),
            effect: None,
            ci: None,
            n,
            secondary: None,
            verdict: Verdict::NotEnoughData,
            plain: format!("Not enough data yet: {reason}."),
            assumptions: String::new(),
        }
    }
}

/// Applies Benjamini–Hochberg within each family and sets verdicts from adjusted p.
pub fn adjust_families(rows: &mut [TestRow]) {
    let mut families: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        if row.p.is_some() {
            families.entry(row.family.clone()).or_default().push(i);
        }
    }
    for indices in families.values() {
        let ps: Vec<f64> = indices.iter().filter_map(|&i| rows[i].p).collect();
        let adjusted = crate::stats::describe::benjamini_hochberg(&ps);
        for (&i, adj) in indices.iter().zip(adjusted) {
            rows[i].p_adjusted = Some(adj);
            if rows[i].verdict != Verdict::NotEnoughData {
                rows[i].verdict = Verdict::from_adjusted_p(Some(adj));
            }
        }
    }
}

/// Empirical-Bayes beta-binomial shrinkage of shares (e.g. CF%) toward a common mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BetaPrior {
    pub mean: f64,
    /// Prior "pseudo-attempts": higher means stronger pull to the mean.
    pub strength: f64,
}

const MIN_PRIOR_STRENGTH: f64 = 4.0;
const MAX_PRIOR_STRENGTH: f64 = 400.0;

/// Method-of-moments fit over `(successes, trials)`; falls back to strong pooling when the
/// observed spread is no larger than binomial noise.
#[must_use]
pub fn fit_beta_prior(observations: &[(f64, f64)]) -> Option<BetaPrior> {
    let usable: Vec<(f64, f64)> = observations.iter().copied().filter(|(_, n)| *n > 0.0).collect();
    let total_trials: f64 = usable.iter().map(|(_, n)| n).sum();
    if total_trials <= 0.0 {
        return None;
    }
    let mean = usable.iter().map(|(s, _)| s).sum::<f64>() / total_trials;
    if usable.len() < 3 || mean <= 0.0 || mean >= 1.0 {
        return Some(BetaPrior {
            mean,
            strength: MAX_PRIOR_STRENGTH,
        });
    }
    let k = usable.len() as f64;
    let rates: Vec<f64> = usable.iter().map(|(s, n)| s / n).collect();
    let rate_mean = rates.iter().sum::<f64>() / k;
    let observed_variance = rates.iter().map(|r| (r - rate_mean).powi(2)).sum::<f64>() / (k - 1.0);
    let noise = mean * (1.0 - mean) * usable.iter().map(|(_, n)| 1.0 / n).sum::<f64>() / k;
    let between = observed_variance - noise;
    let strength = if between <= 0.0 {
        MAX_PRIOR_STRENGTH
    } else {
        (mean * (1.0 - mean) / between - 1.0).clamp(MIN_PRIOR_STRENGTH, MAX_PRIOR_STRENGTH)
    };
    Some(BetaPrior { mean, strength })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Shrunk {
    /// Posterior mean share, percent.
    pub estimate: Estimate,
    /// Posterior probability the true share exceeds the prior mean.
    pub prob_above_average: f64,
}

/// Posterior Beta(s + κμ, f + κ(1−μ)) summary with a 90% credible interval.
#[must_use]
pub fn shrink(successes: f64, failures: f64, prior: BetaPrior) -> Option<Shrunk> {
    let a = successes + prior.strength * prior.mean;
    let b = failures + prior.strength * (1.0 - prior.mean);
    let mean = a / (a + b);
    let low = dist::beta_quantile(0.05, a, b)?;
    let high = dist::beta_quantile(0.95, a, b)?;
    let below = statrs::function::beta::beta_reg(a, b, prior.mean);
    Some(Shrunk {
        estimate: Estimate {
            value: 100.0 * mean,
            low: 100.0 * low,
            high: 100.0 * high,
        },
        prob_above_average: 1.0 - below,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_samples_are_pulled_toward_the_mean() {
        let prior = BetaPrior {
            mean: 0.5,
            strength: 20.0,
        };
        let tiny = shrink(3.0, 0.0, prior).unwrap();
        let large = shrink(300.0, 0.0, prior).unwrap();
        assert!(tiny.estimate.value < 60.0);
        assert!(large.estimate.value > 90.0);
        assert!(tiny.estimate.low < tiny.estimate.value && tiny.estimate.value < tiny.estimate.high);
    }

    #[test]
    fn identical_rates_pool_completely() {
        let prior = fit_beta_prior(&[(10.0, 20.0), (15.0, 30.0), (20.0, 40.0)]).unwrap();
        assert!((prior.mean - 0.5).abs() < 1e-12);
        assert!((prior.strength - MAX_PRIOR_STRENGTH).abs() < 1e-12);
    }

    #[test]
    fn spread_beyond_noise_lowers_prior_strength() {
        let prior = fit_beta_prior(&[(90.0, 100.0), (10.0, 100.0), (50.0, 100.0), (70.0, 100.0)]).unwrap();
        assert!(prior.strength < 20.0, "{}", prior.strength);
    }
}
