//! Hypothesis tests behind the plain-English verdicts, and "games needed" power estimates.

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::common::{TestRow, Verdict};
use crate::analysis::impact::{Observation, interaction_test, unit_observations};
use crate::analysis::pairs::PairRow;
use crate::analysis::passing::PassingReport;
use crate::analysis::team::TeamReport;
use crate::analysis::units::{UnitRow, UnitsReport};
use crate::model::{GameId, UnitKind};
use crate::stats::dist;
use crate::stats::glm::{PoissonProblem, fit_poisson};
use crate::stats::hypothesis::{chi_square_independence, fisher_exact, paired_t, wilcoxon_signed_rank};
use crate::stats::random::{self, Rng};

const MIN_ATTEMPTS: u32 = 5;
const MIN_GAMES_PAIRED: usize = 3;

const fn kind_label(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::DefencePair => "defence pairs",
        UnitKind::ForwardLine => "forward lines",
        UnitKind::FullUnit => "five-man units",
        UnitKind::PowerPlay => "power-play units",
        UnitKind::PenaltyKill => "penalty-kill units",
    }
}

fn names(unit: &UnitRow) -> String {
    unit.players.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(" – ")
}

/// "Is chemistry real overall?": additive player model vs one with a term per group.
fn chemistry_overall(context: &Context<'_>, kind: UnitKind) -> TestRow {
    let family = "Chemistry overall";
    let question = format!("Do {} perform differently than their individual players add up to?", kind_label(kind));
    let method = "Poisson likelihood-ratio (deviance) test: player-only model vs player + unit terms, shot attempts for and against";
    let observations = unit_observations(context, kind);
    let games = distinct_games(&observations);
    if games < 2 {
        return TestRow::not_enough(family, &question, method, format!("{games} game"), "needs the same units seen across at least 2 games");
    }
    let Some(test) = interaction_test(&observations) else {
        return TestRow::not_enough(family, &question, method, format!("{} observations", observations.len()), "no unit was seen often enough to separate it from its players");
    };
    let p = dist::chi_square_sf(test.delta_deviance, test.df);
    let quasi_f = (test.residual_df > 0.0 && test.residual_deviance > 0.0).then(|| {
        let dispersion = test.residual_deviance / test.residual_df;
        let f = (test.delta_deviance / test.df) / dispersion;
        (f, dist::f_sf(f, test.df, test.residual_df))
    });
    let plain = match p {
        Some(p) if p < 0.05 => format!("Some {} really are more (or less) than the sum of their parts.", kind_label(kind)),
        Some(_) => format!("So far, {} play about as well as their individual players predict; no clear chemistry effect yet.", kind_label(kind)),
        None => "Could not compute.".to_owned(),
    };
    TestRow {
        family: family.to_owned(),
        question,
        method: method.to_owned(),
        statistic_label: "χ² (ΔDeviance)".to_owned(),
        statistic: Some(test.delta_deviance),
        df: Some(format!("{:.0}", test.df)),
        p,
        p_adjusted: None,
        effect_label: String::new(),
        effect: None,
        ci: None,
        n: format!("{} unit-games, {} units, {games} games", test.observations, test.groups),
        secondary: quasi_f.map(|(f, p)| {
            format!(
                "Over-dispersion-adjusted: F({:.0}, {:.0}) = {f:.2}, p = {}",
                test.df,
                test.residual_df,
                p.map_or_else(|| "—".to_owned(), |p| format!("{p:.3}"))
            )
        }),
        verdict: Verdict::from_adjusted_p(p),
        plain,
        assumptions: "Counts are Poisson given time on ice; the over-dispersion-adjusted F is the safer read. Uses InStat's listed combinations only.".to_owned(),
    }
}

fn distinct_games(observations: &[Observation]) -> usize {
    let mut games: Vec<&GameId> = observations.iter().map(|o| &o.game).collect();
    games.sort();
    games.dedup();
    games.len()
}

/// ANOVA-style deviance test: do units differ once each game's overall level is allowed for?
fn units_differ(context: &Context<'_>, kind: UnitKind) -> TestRow {
    let family = "Do lines differ?";
    let question = format!("Do our {} really differ in shot attempts for and against?", kind_label(kind));
    let method = "Poisson deviance test with game as a block: game-only model vs game + unit";
    let observations = unit_observations(context, kind);
    let mut units: Vec<Vec<crate::model::PlayerId>> = observations
        .iter()
        .map(|o| {
            let mut p = o.players.clone();
            p.sort();
            p
        })
        .collect();
    units.sort();
    units.dedup();
    let mut games: Vec<GameId> = observations.iter().map(|o| o.game.clone()).collect();
    games.sort();
    games.dedup();
    if units.len() < 2 || observations.len() < 3 {
        return TestRow::not_enough(family, &question, method, format!("{} units", units.len()), "needs at least two units");
    }
    let build = |with_units: bool| {
        let columns = games.len() + if with_units { units.len() - 1 } else { 0 };
        DMatrix::from_fn(observations.len(), columns, |i, j| {
            let o = &observations[i];
            if j < games.len() {
                return f64::from(u8::from(o.game == games[j]));
            }
            let mut p = o.players.clone();
            p.sort();
            f64::from(u8::from(p == units[j - games.len() + 1]))
        })
    };
    let offset = DVector::from_iterator(observations.len(), observations.iter().map(|o| (o.toi.0 / 3600.0).max(1e-6).ln()));
    let mut delta = 0.0;
    let mut df = 0.0;
    for counts in [
        observations.iter().map(|o| o.count_for).collect::<Vec<_>>(),
        observations.iter().map(|o| o.count_against).collect::<Vec<_>>(),
    ] {
        let y = DVector::from_vec(counts);
        let fit = |x: DMatrix<f64>| {
            let p = x.ncols();
            fit_poisson(&PoissonProblem { x, y: y.clone(), offset: offset.clone(), penalty: DVector::from_element(p, 1e-4) })
        };
        let (Some(small), Some(large)) = (fit(build(false)), fit(build(true))) else {
            return TestRow::not_enough(family, &question, method, String::new(), "the model could not be fitted");
        };
        delta += (small.deviance - large.deviance).max(0.0);
        df += crate::analysis::impact::rank(&build(true)).saturating_sub(crate::analysis::impact::rank(&build(false))) as f64;
    }
    if df <= 0.0 {
        return TestRow::not_enough(family, &question, method, String::new(), "units cannot be separated from games yet");
    }
    let p = dist::chi_square_sf(delta, df);
    TestRow {
        family: family.to_owned(),
        question,
        method: method.to_owned(),
        statistic_label: "χ² (ΔDeviance)".to_owned(),
        statistic: Some(delta),
        df: Some(format!("{df:.0}")),
        p,
        p_adjusted: None,
        effect_label: String::new(),
        effect: None,
        ci: None,
        n: format!("{} unit-games, {} units, {} games", observations.len(), units.len(), games.len()),
        secondary: None,
        verdict: Verdict::from_adjusted_p(p),
        plain: match p {
            Some(p) if p < 0.05 => format!("Yes: the gaps between our {} are bigger than chance.", kind_label(kind)),
            _ => format!("Not clearly: the gaps between our {} could still be chance.", kind_label(kind)),
        },
        assumptions: "Poisson counts given time on ice; small units contribute little information.".to_owned(),
    }
}

/// Each unit's CF% against the rest of the team in the same games (exact binomial).
fn unit_vs_team(context: &Context<'_>, unit: &UnitRow) -> Option<TestRow> {
    let n = unit.corsi_for + unit.corsi_against;
    let too_short = unit.toi.0 < context.min_unit_toi.0;
    if too_short || n < MIN_ATTEMPTS {
        return None;
    }
    let games: Vec<&GameId> = unit.per_game.iter().map(|g| &g.game).collect();
    let (team_for, team_against) = context
        .scope
        .iter()
        .filter(|g| games.contains(&&g.id))
        .fold((0.0, 0.0), |(f, a), g| {
            (f + f64::from(g.summary.even_strength_shots.0), a + f64::from(g.opponent_summary.even_strength_shots.0))
        });
    let rest_for = (team_for - f64::from(unit.corsi_for)).max(0.0);
    let rest_against = (team_against - f64::from(unit.corsi_against)).max(0.0);
    if rest_for + rest_against <= 0.0 {
        return None;
    }
    let rest_share = rest_for / (rest_for + rest_against);
    let p = dist::binomial_exact_two_sided(u64::from(unit.corsi_for), u64::from(n), rest_share);
    let share = 100.0 * f64::from(unit.corsi_for) / f64::from(n);
    Some(TestRow {
        family: format!("{} vs team", kind_label(unit.kind)),
        question: format!("Is {} better or worse than the rest of the team?", names(unit)),
        method: "Exact binomial test of the unit's shot-attempt share against the team's share without it".to_owned(),
        statistic_label: "CF / attempts".to_owned(),
        statistic: Some(f64::from(unit.corsi_for)),
        df: Some(format!("n = {n}")),
        p: Some(p),
        p_adjusted: None,
        effect_label: "CF% − rest of team".to_owned(),
        effect: Some(share - 100.0 * rest_share),
        ci: crate::stats::describe::wilson_interval(f64::from(unit.corsi_for), f64::from(n), 0.95)
            .map(|(l, h)| (100.0 * l - 100.0 * rest_share, 100.0 * h - 100.0 * rest_share)),
        n: format!("{:.0} min, {} games", unit.toi.minutes(), unit.games),
        secondary: None,
        verdict: Verdict::from_adjusted_p(Some(p)),
        plain: format!(
            "{} controls {share:.0}% of shot attempts vs {:.0}% for the rest of the team.",
            names(unit),
            100.0 * rest_share
        ),
        assumptions: "Attempts treated as independent; the team share excluding the unit is treated as fixed.".to_owned(),
    })
}

/// Pair's CF% together vs apart, game by game (paired t and Wilcoxon).
fn pair_with_without(pair: &PairRow) -> Option<TestRow> {
    let usable: Vec<(f64, f64)> = pair
        .per_game
        .iter()
        .filter_map(|g| {
            let together = crate::analysis::common::share_pct(f64::from(g.corsi_for), f64::from(g.corsi_against))?;
            let apart = crate::analysis::common::share_pct(
                f64::from(g.a_apart_for + g.b_apart_for),
                f64::from(g.a_apart_against + g.b_apart_against),
            )?;
            (g.corsi_for + g.corsi_against >= 2).then_some((together, apart))
        })
        .collect();
    let pair_name = format!("{} & {}", pair.a.name, pair.b.name);
    let question = format!("Do {pair_name} control more shots together than apart?");
    let family = "Pair with vs without";
    let method = "Paired t-test by game, with a Wilcoxon signed-rank check";
    if usable.len() < MIN_GAMES_PAIRED {
        return Some(TestRow::not_enough(family, &question, method, format!("{} games", usable.len()), "needs at least 3 games with time both together and apart"));
    }
    let (together, apart): (Vec<f64>, Vec<f64>) = usable.iter().copied().unzip();
    let t = paired_t(&together, &apart)?;
    let diffs: Vec<f64> = usable.iter().map(|(a, b)| a - b).collect();
    let wilcoxon = wilcoxon_signed_rank(&diffs);
    Some(TestRow {
        family: family.to_owned(),
        question,
        method: method.to_owned(),
        statistic_label: "t".to_owned(),
        statistic: Some(t.t),
        df: Some(format!("{:.0}", t.df)),
        p: Some(t.p),
        p_adjusted: None,
        effect_label: "Mean CF% difference (Cohen's d_z)".to_owned(),
        effect: Some(t.mean_difference),
        ci: Some(t.ci),
        n: format!("{} games", usable.len()),
        secondary: wilcoxon.map(|w| format!("Wilcoxon signed-rank W = {:.1}, p = {:.3}", w.statistic, w.p)),
        verdict: Verdict::from_adjusted_p(Some(t.p)),
        plain: format!(
            "Together they average {:+.1} percentage points of shot share compared with their games apart (d = {:.2}).",
            t.mean_difference, t.effect_size
        ),
        assumptions: "Per-game differences roughly normal (t-test); Wilcoxon given as a distribution-free check.".to_owned(),
    })
}

/// Pair's shot share against what the additive model predicts (conditional binomial).
fn pair_vs_expectation(pair: &PairRow) -> Option<TestRow> {
    let corsi = pair.corsi.as_ref()?;
    let expected = corsi.expected_pct?;
    let n = corsi.corsi_for + corsi.corsi_against;
    if n < MIN_ATTEMPTS {
        return None;
    }
    let p = dist::binomial_exact_two_sided(u64::from(corsi.corsi_for), u64::from(n), expected / 100.0);
    let observed = 100.0 * f64::from(corsi.corsi_for) / f64::from(n);
    Some(TestRow {
        family: "Pair chemistry vs expectation".to_owned(),
        question: format!("Are {} & {} better together than their individual ratings predict?", pair.a.name, pair.b.name),
        method: "Exact binomial test of observed shot share vs the player-only model's expected share".to_owned(),
        statistic_label: "CF / attempts".to_owned(),
        statistic: Some(f64::from(corsi.corsi_for)),
        df: Some(format!("n = {n}")),
        p: Some(p),
        p_adjusted: None,
        effect_label: "Observed − expected CF%".to_owned(),
        effect: Some(observed - expected),
        ci: None,
        n: format!("{:.0} min together", corsi.toi.minutes()),
        secondary: None,
        verdict: Verdict::from_adjusted_p(Some(p)),
        plain: format!("Together {observed:.0}% of shot attempts; their individual ratings predict {expected:.0}%."),
        assumptions: "Expected share comes from ratings fitted on the same data, which makes this test conservative.".to_owned(),
    })
}

fn chi_square_row(family: &str, question: &str, table: &[Vec<f64>], labels: &str, plain_yes: &str, plain_no: &str) -> TestRow {
    let method = "Chi-square test of independence";
    let Some(result) = chi_square_independence(table) else {
        return TestRow::not_enough(family, question, method, String::new(), "not enough counts");
    };
    let total: f64 = table.iter().flatten().sum();
    let (p, method_used) = if table.len() == 2 && table[0].len() == 2 && result.min_expected < 5.0 {
        let cell = |r: usize, c: usize| table[r][c].round().max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let p = fisher_exact(cell(0, 0) as u64, cell(0, 1) as u64, cell(1, 0) as u64, cell(1, 1) as u64);
        (p, "Fisher exact test (small expected counts)")
    } else {
        (result.p, method)
    };
    TestRow {
        family: family.to_owned(),
        question: question.to_owned(),
        method: method_used.to_owned(),
        statistic_label: if result.corrected { "χ² (Yates)" } else { "χ²" }.to_owned(),
        statistic: Some(result.statistic),
        df: Some(format!("{:.0}", result.df)),
        p: Some(p),
        p_adjusted: None,
        effect_label: "Cramér's V".to_owned(),
        effect: Some(result.cramers_v),
        ci: None,
        n: format!("{total:.0} {labels}"),
        secondary: None,
        verdict: Verdict::from_adjusted_p(Some(p)),
        plain: if p < 0.05 { plain_yes.to_owned() } else { plain_no.to_owned() },
        assumptions: if result.min_expected < 5.0 {
            format!("Smallest expected count {:.1} (< 5): treat with caution.", result.min_expected)
        } else {
            "Counts independent; expected counts ≥ 5.".to_owned()
        },
    }
}

fn team_tables(team: &TeamReport) -> Vec<TestRow> {
    let faceoffs: Vec<Vec<f64>> = team
        .faceoffs
        .iter()
        .map(|z| vec![f64::from(z.won), f64::from(z.lost)])
        .collect();
    let shots: Vec<Vec<f64>> = team
        .periods
        .iter()
        .map(|p| vec![f64::from(p.shots_for), f64::from(p.shots_against)])
        .collect();
    vec![
        chi_square_row(
            "Team patterns",
            "Does our faceoff success depend on the zone?",
            &faceoffs,
            "faceoffs",
            "Yes: faceoff results differ by zone more than chance would explain.",
            "No clear zone effect on faceoffs.",
        ),
        chi_square_row(
            "Team patterns",
            "Does our share of shots change from period to period?",
            &shots,
            "shots",
            "Yes: our shot share really changes between periods.",
            "Shot share by period is within normal variation.",
        ),
    ]
}

fn passing_row(passing: &PassingReport) -> TestRow {
    let family = "Team patterns";
    let question = "Do players pass to particular teammates more than their passing volume explains?";
    let method = "G² test of quasi-independence (self-passes excluded)";
    let Some((g, df, p)) = passing.quasi_independence else {
        return TestRow::not_enough(family, question, method, format!("{} passes", passing.total), "needs at least 20 passes among 3+ players");
    };
    TestRow {
        family: family.to_owned(),
        question: question.to_owned(),
        method: method.to_owned(),
        statistic_label: "G²".to_owned(),
        statistic: Some(g),
        df: Some(format!("{df:.0}")),
        p: Some(p),
        p_adjusted: None,
        effect_label: "Reciprocity".to_owned(),
        effect: passing.reciprocity,
        ci: None,
        n: format!("{} passes", passing.total),
        secondary: None,
        verdict: Verdict::from_adjusted_p(Some(p)),
        plain: if p < 0.05 {
            "Yes: there are real passing connections beyond chance.".to_owned()
        } else {
            "Passing partners look close to random given each player's volume.".to_owned()
        },
        assumptions: "Many cells are small; the G² approximation is rough until more games are loaded.".to_owned(),
    }
}

/// How many games until an observed share difference would be detected 80% of the time.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerRow {
    pub subject: String,
    pub metric: String,
    pub assumed_share: f64,
    pub baseline_share: f64,
    pub attempts_per_game: f64,
    pub games_so_far: u32,
    pub games_needed: Option<u32>,
    pub note: String,
}

const POWER_SIMULATIONS: usize = 400;
const POWER_TARGET: f64 = 0.8;
const MAX_GAMES: u32 = 60;

fn binomial_draw(rng: &mut Rng, n: u64, p: f64) -> u64 {
    (0..n).filter(|_| random::uniform(rng) < p).count() as u64
}

/// Monte Carlo power of the exact binomial test after `games` games.
fn power(rng: &mut Rng, attempts_per_game: f64, share: f64, baseline: f64, games: u32) -> f64 {
    let hits = (0..POWER_SIMULATIONS)
        .filter(|_| {
            let n: u64 = (0..games).map(|_| random::poisson(rng, attempts_per_game)).sum();
            if n == 0 {
                return false;
            }
            let k = binomial_draw(rng, n, share);
            dist::binomial_exact_two_sided(k, n, baseline) < 0.05
        })
        .count();
    hits as f64 / POWER_SIMULATIONS as f64
}

fn games_needed(rng: &mut Rng, attempts_per_game: f64, share: f64, baseline: f64) -> Option<u32> {
    if (share - baseline).abs() < 0.005 || attempts_per_game <= 0.0 {
        return None;
    }
    let mut low = 1;
    let mut high = MAX_GAMES;
    if power(rng, attempts_per_game, share, baseline, high) < POWER_TARGET {
        return None;
    }
    while low < high {
        let middle = u32::midpoint(low, high);
        if power(rng, attempts_per_game, share, baseline, middle) >= POWER_TARGET {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some(low)
}

/// Uses the shrunk share as the "true" effect so small samples do not promise quick answers.
fn unit_power(rng: &mut Rng, unit: &UnitRow, baseline: f64) -> Option<PowerRow> {
    let shrunk = unit.shrunk_corsi?;
    let share = shrunk.estimate.value / 100.0;
    let attempts_per_game = f64::from(unit.corsi_for + unit.corsi_against) / f64::from(unit.games.max(1));
    let needed = games_needed(rng, attempts_per_game, share, baseline);
    Some(PowerRow {
        subject: names(unit),
        metric: "Shot-attempt share".to_owned(),
        assumed_share: 100.0 * share,
        baseline_share: 100.0 * baseline,
        attempts_per_game,
        games_so_far: unit.games,
        games_needed: needed,
        note: needed.map_or_else(
            || "Difference too small to confirm within 60 games at this ice time.".to_owned(),
            |n| {
                if n <= unit.games {
                    "Already enough games to detect a difference this size.".to_owned()
                } else {
                    format!("About {} more games at this usage to confirm (80% power).", n - unit.games)
                }
            },
        ),
    })
}

pub struct Significance {
    pub tests: Vec<TestRow>,
    pub power: Vec<PowerRow>,
}

#[must_use]
pub fn significance(context: &Context<'_>, units: &UnitsReport, pairs: &[PairRow], passing: &PassingReport, team: &TeamReport) -> Significance {
    let mut tests = Vec::new();
    for kind in [UnitKind::DefencePair, UnitKind::ForwardLine, UnitKind::FullUnit] {
        tests.push(chemistry_overall(context, kind));
        tests.push(units_differ(context, kind));
    }
    for list in [&units.defence_pairs, &units.forward_lines, &units.full_units] {
        tests.extend(list.iter().filter_map(|u| unit_vs_team(context, u)));
    }
    let eligible_pairs: Vec<&PairRow> = pairs
        .iter()
        .filter(|p| p.corsi.as_ref().is_some_and(|c| c.toi.0 >= context.min_unit_toi.0))
        .collect();
    let with_without: Vec<TestRow> = eligible_pairs.iter().filter_map(|p| pair_with_without(p)).collect();
    if with_without.iter().all(|r| r.verdict == Verdict::NotEnoughData) {
        tests.push(TestRow::not_enough(
            "Pair with vs without",
            "Do pairs control more shots together than apart?",
            "Paired t-test by game (with Wilcoxon signed-rank)",
            format!("{} games loaded", context.scope.len()),
            "needs at least 3 games in which a pair played both together and apart",
        ));
    } else {
        tests.extend(with_without.into_iter().filter(|r| r.verdict != Verdict::NotEnoughData));
    }
    tests.extend(eligible_pairs.iter().filter_map(|p| pair_vs_expectation(p)));
    tests.extend(team_tables(team));
    tests.push(passing_row(passing));

    let mut rng = random::seeded(2024);
    let mut power = Vec::new();
    let baselines: HashMap<UnitKind, f64> = units.priors.iter().map(|(k, p)| (*k, p.mean)).collect();
    for list in [&units.defence_pairs, &units.forward_lines] {
        for unit in list.iter().filter(|u| u.toi.0 >= context.min_unit_toi.0) {
            let Some(&baseline) = baselines.get(&unit.kind) else {
                continue;
            };
            power.extend(unit_power(&mut rng, unit, baseline));
        }
    }
    Significance { tests, power }
}
