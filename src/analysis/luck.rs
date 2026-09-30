//! Did results match the chances?
//!
//! Exact win probabilities from every shot's xG, expected against actual standings points, a
//! Pythagorean record, and each game's goal differential split into shot volume, shot
//! quality, finishing and goaltending.

use serde::Serialize;

use crate::analysis::Context;
use crate::analysis::team::{Outcome, outcome};
use crate::model::{Date, Game, GameId};

/// Poisson goal counts beyond this carry negligible probability for any real game's xG.
const MAX_POISSON_GOALS: usize = 30;
const REGULATION_MINUTES: f64 = 60.0;
/// Sudden-death overtime (3-on-3); still level after it, the game is a tie.
const OVERTIME_MINUTES: f64 = 5.0;
/// The usual hockey rule of thumb for turning goals for and against into a win share.
const PYTHAGOREAN_EXPONENT: f64 = 2.0;

/// P(k goals) for k = 0, 1, 2, …
pub(crate) type GoalDistribution = Vec<f64>;

/// Exact distribution of the number of goals when each shot scores independently with its
/// own chance (the Poisson-binomial distribution).
pub(crate) fn goals_from_shots(chances: &[f64]) -> GoalDistribution {
    let mut distribution = vec![0.0; chances.len() + 1];
    distribution[0] = 1.0;
    for (n, &p) in chances.iter().enumerate() {
        let p = p.clamp(0.0, 1.0);
        for k in (1..=n + 1).rev() {
            distribution[k] = distribution[k] * (1.0 - p) + distribution[k - 1] * p;
        }
        distribution[0] *= 1.0 - p;
    }
    distribution
}

/// Poisson goals with mean `xg`, for games with a total but no per-shot values.
fn goals_from_total(xg: f64) -> GoalDistribution {
    let mut distribution = Vec::with_capacity(MAX_POISSON_GOALS + 1);
    let mut term = (-xg).exp();
    for k in 0..=MAX_POISSON_GOALS {
        distribution.push(term);
        term *= xg / (k as f64 + 1.0);
    }
    distribution
}

/// Chances of each result; they add up to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Deserved {
    /// In regulation or overtime.
    pub win: f64,
    pub overtime_loss: f64,
    /// Level after overtime.
    pub tie: f64,
    pub loss: f64,
    pub expected_points: f64,
}

impl Deserved {
    /// P(0, 1, 2 points).
    const fn points_distribution(self) -> [f64; 3] {
        [self.loss, self.overtime_loss + self.tie, self.win]
    }
}

/// Sudden-death overtime when each side scores at its regulation xG rate: whoever scores
/// first wins. 3-on-3 usually scores faster than that, so ties come out slightly too likely.
/// Returns (P(we win), P(they win)); the rest is a tie.
fn overtime(xg_for: f64, xg_against: f64) -> (f64, f64) {
    let total = xg_for + xg_against;
    if total <= 0.0 {
        return (0.0, 0.0);
    }
    let decided = 1.0 - (-total * OVERTIME_MINUTES / REGULATION_MINUTES).exp();
    (decided * xg_for / total, decided * xg_against / total)
}

fn deserved(ours: &GoalDistribution, theirs: &GoalDistribution) -> Deserved {
    let (mut win, mut level, mut loss) = (0.0, 0.0, 0.0);
    for (a, pa) in ours.iter().enumerate() {
        for (b, pb) in theirs.iter().enumerate() {
            let p = pa * pb;
            match a.cmp(&b) {
                std::cmp::Ordering::Greater => win += p,
                std::cmp::Ordering::Equal => level += p,
                std::cmp::Ordering::Less => loss += p,
            }
        }
    }
    let (overtime_win, overtime_loss) = overtime(mean_goals(ours), mean_goals(theirs));
    let result = Deserved {
        win: win + level * overtime_win,
        overtime_loss: level * overtime_loss,
        tie: level * (1.0 - overtime_win - overtime_loss),
        loss,
        expected_points: 0.0,
    };
    let [_, one, two] = result.points_distribution();
    Deserved { expected_points: one + 2.0 * two, ..result }
}

fn mean_goals(distribution: &GoalDistribution) -> f64 {
    distribution.iter().enumerate().map(|(k, p)| k as f64 * p).sum()
}

/// A game's goal differential as the sum of four parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct GoalSplit {
    /// Taking more shot attempts than the opponent, at the game's average chance quality.
    pub volume: f64,
    /// Better average chance per attempt than the opponent, at the game's average volume.
    pub quality: f64,
    /// Our goals minus our xG.
    pub finishing: f64,
    /// Their xG minus their goals.
    pub goaltending: f64,
}

impl GoalSplit {
    fn add(&mut self, other: Self) {
        self.volume += other.volume;
        self.quality += other.quality;
        self.finishing += other.finishing;
        self.goaltending += other.goaltending;
    }
}

/// One team's side of a game: goals, attempts and expected goals.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Side {
    goals: u32,
    attempts: u32,
    xg: f64,
}

impl Side {
    fn per_attempt(self) -> f64 {
        if self.attempts == 0 { 0.0 } else { self.xg / f64::from(self.attempts) }
    }
}

fn split(ours: Side, theirs: Side) -> GoalSplit {
    let (nf, na) = (f64::from(ours.attempts), f64::from(theirs.attempts));
    let (qf, qa) = (ours.per_attempt(), theirs.per_attempt());
    GoalSplit {
        volume: (nf - na) * (qf + qa) / 2.0,
        quality: (qf - qa) * (nf + na) / 2.0,
        finishing: f64::from(ours.goals) - ours.xg,
        goaltending: theirs.xg - f64::from(theirs.goals),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum XgSource {
    /// Every charted shot's own xG.
    Shots,
    /// Only the team totals (no shooting chart), treated as Poisson.
    Totals,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameLuck {
    pub game: GameId,
    pub date: Date,
    pub opponent: String,
    pub goals_for: u32,
    pub goals_against: u32,
    pub outcome: Outcome,
    pub points: u32,
    pub xg_for: f64,
    pub xg_against: f64,
    pub source: XgSource,
    pub deserved: Deserved,
    pub split: GoalSplit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LuckReport {
    pub games: Vec<GameLuck>,
    pub points: u32,
    pub expected_points: f64,
    /// P(total points = k), for k = 0, 1, 2, …
    pub points_distribution: Vec<f64>,
    /// How often the chances we created and allowed would earn no more points than we have.
    pub chance_of_at_most_actual: Option<f64>,
    pub chance_of_at_least_actual: Option<f64>,
    /// Expected share of games won from goals, and from xG.
    pub pythagorean_goals: Option<f64>,
    pub pythagorean_xg: Option<f64>,
    pub split: GoalSplit,
}

/// Our and their per-shot xG from the shooting charts, when every shot has a value.
fn shot_chances(context: &Context<'_>, game: &Game) -> Option<(Vec<f64>, Vec<f64>)> {
    if game.charted_shots.is_empty() && game.charted_shots_against.is_empty() {
        return None;
    }
    let ours: Option<Vec<f64>> = (0..game.charted_shots.len()).map(|i| context.shot_xg.ours(&game.id, i)).collect();
    let theirs: Option<Vec<f64>> = (0..game.charted_shots_against.len())
        .map(|i| context.shot_xg.theirs(&game.id, i))
        .collect();
    ours.zip(theirs)
}

fn game_luck(context: &Context<'_>, game: &Game) -> Option<GameLuck> {
    let (source, ours, theirs) = if let Some((ours, theirs)) = shot_chances(context, game) {
        (XgSource::Shots, goals_from_shots(&ours), goals_from_shots(&theirs))
    } else {
        let (xg_for, xg_against) = game.summary.xg.zip(game.opponent_summary.xg)?;
        (XgSource::Totals, goals_from_total(xg_for), goals_from_total(xg_against))
    };
    let (xg_for, xg_against) = (mean_goals(&ours), mean_goals(&theirs));
    let result = outcome(game);
    Some(GameLuck {
        game: game.id.clone(),
        date: game.date,
        opponent: game.opponent.0.clone(),
        goals_for: game.goals_for,
        goals_against: game.goals_against,
        outcome: result,
        points: result.points(),
        xg_for,
        xg_against,
        source,
        deserved: deserved(&ours, &theirs),
        split: split(
            Side { goals: game.goals_for, attempts: game.summary.shots, xg: xg_for },
            Side { goals: game.goals_against, attempts: game.opponent_summary.shots, xg: xg_against },
        ),
    })
}

/// Distribution of season points when each game's points are drawn independently.
fn season_points(games: &[GameLuck]) -> Vec<f64> {
    games.iter().fold(vec![1.0], |total, game| {
        let per_game = game.deserved.points_distribution();
        let mut next = vec![0.0; total.len() + per_game.len() - 1];
        for (a, pa) in total.iter().enumerate() {
            for (b, pb) in per_game.iter().enumerate() {
                next[a + b] += pa * pb;
            }
        }
        next
    })
}

fn pythagorean(scored: f64, allowed: f64) -> Option<f64> {
    let (s, a) = (scored.powf(PYTHAGOREAN_EXPONENT), allowed.powf(PYTHAGOREAN_EXPONENT));
    (s + a > 0.0).then(|| s / (s + a))
}

#[must_use]
pub fn luck(context: &Context<'_>) -> LuckReport {
    let games: Vec<GameLuck> = context.scope.iter().filter_map(|g| game_luck(context, g)).collect();
    let points: u32 = games.iter().map(|g| g.points).sum();
    let distribution = season_points(&games);
    let actual = usize::try_from(points).unwrap_or(usize::MAX);
    let has_games = !games.is_empty();
    let goals_for: u32 = games.iter().map(|g| g.goals_for).sum();
    let goals_against: u32 = games.iter().map(|g| g.goals_against).sum();
    let xg_for: f64 = games.iter().map(|g| g.xg_for).sum();
    let xg_against: f64 = games.iter().map(|g| g.xg_against).sum();
    LuckReport {
        points,
        expected_points: games.iter().map(|g| g.deserved.expected_points).sum(),
        chance_of_at_most_actual: has_games.then(|| distribution.iter().take(actual + 1).sum()),
        chance_of_at_least_actual: has_games.then(|| distribution.iter().skip(actual).sum()),
        points_distribution: distribution,
        pythagorean_goals: pythagorean(f64::from(goals_for), f64::from(goals_against)),
        pythagorean_xg: pythagorean(xg_for, xg_against),
        split: games.iter().fold(GoalSplit::default(), |mut total, g| {
            total.add(g.split);
            total
        }),
        games,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn certain_and_impossible_shots_give_certain_goal_counts() {
        let d = goals_from_shots(&[1.0, 0.0, 1.0]);
        assert!(close(d[2], 1.0));
        assert!(close(d.iter().sum(), 1.0));
    }

    #[test]
    fn two_even_chances_match_the_binomial() {
        let d = goals_from_shots(&[0.5, 0.5]);
        assert!(close(d[0], 0.25) && close(d[1], 0.5) && close(d[2], 0.25));
    }

    #[test]
    fn identical_chances_on_both_sides_are_a_coin_flip() {
        let shots = goals_from_shots(&[0.1, 0.3, 0.05, 0.2]);
        let result = deserved(&shots, &shots);
        assert!(close(result.win, result.loss + result.overtime_loss));
        assert!(close(result.win + result.overtime_loss + result.tie + result.loss, 1.0));
        assert!(result.tie > 0.0 && result.overtime_loss > 0.0);
    }

    #[test]
    fn overtime_goes_to_the_side_that_creates_more() {
        let (ours, theirs) = overtime(4.0, 1.0);
        assert!(close(ours / theirs, 4.0));
        assert!(close(ours + theirs, 1.0 - (-5.0_f64 / 12.0).exp()));
    }

    #[test]
    fn overtime_without_chances_ends_level() {
        assert_eq!(overtime(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn poisson_totals_match_their_mean() {
        let d = goals_from_total(3.2);
        let mean: f64 = d.iter().enumerate().map(|(k, p)| k as f64 * p).sum();
        assert!((mean - 3.2).abs() < 1e-6);
    }

    #[test]
    fn goal_split_adds_up_to_the_goal_differential() {
        let parts = split(Side { goals: 4, attempts: 44, xg: 2.12 }, Side { goals: 5, attempts: 87, xg: 5.47 });
        let total = parts.volume + parts.quality + parts.finishing + parts.goaltending;
        assert!(close(total, -1.0), "{total}");
        assert!(parts.volume < 0.0);
    }

    #[test]
    fn season_points_distribution_has_the_expected_mean() {
        let game = |win: f64, tie: f64| GameLuck {
            game: GameId(String::new()),
            date: Date { year: 2026, month: 9, day: 1 },
            opponent: String::new(),
            goals_for: 0,
            goals_against: 0,
            outcome: Outcome::Loss,
            points: 0,
            xg_for: 0.0,
            xg_against: 0.0,
            source: XgSource::Shots,
            deserved: Deserved { win, overtime_loss: 0.0, tie, loss: 1.0 - win - tie, expected_points: 2.0 * win + tie },
            split: GoalSplit::default(),
        };
        let games = [game(0.5, 0.2), game(0.1, 0.3), game(0.7, 0.1)];
        let distribution = season_points(&games);
        let mean: f64 = distribution.iter().enumerate().map(|(k, p)| k as f64 * p).sum();
        let expected: f64 = games.iter().map(|g| g.deserved.expected_points).sum();
        assert!(close(distribution.iter().sum(), 1.0));
        assert!(close(mean, expected), "{mean} vs {expected}");
        assert_eq!(distribution.len(), 7);
    }
}
