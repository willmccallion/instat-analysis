//! Head-to-head puck battles between our skaters and the opponent's, per game and per player.

use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::common::PlayerRef;
use crate::model::{Date, Game, GameId, Matchup, Opponent, PlayerId};

/// Puck battles from our side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Battles {
    pub won: u32,
    pub lost: u32,
}

impl Battles {
    const fn of(matchup: &Matchup) -> Self {
        Self {
            won: matchup.battles_won,
            lost: matchup.battles_lost,
        }
    }

    const fn add(&mut self, other: Self) {
        self.won += other.won;
        self.lost += other.lost;
    }

    const fn total(self) -> u32 {
        self.won + self.lost
    }
}

/// One opponent skater's battles against us in a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpponentBattles {
    pub opponent: Opponent,
    pub battles: Battles,
}

/// Battles between `players[player]` and `opponents[opponent]` of a [`GameMatchups`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchupCell {
    pub player: usize,
    pub opponent: usize,
    pub battles: Battles,
}

/// Who battled whom in one game; players and opponents are listed busiest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GameMatchups {
    pub players: Vec<PlayerRef>,
    pub opponents: Vec<OpponentBattles>,
    pub cells: Vec<MatchupCell>,
}

/// One opponent skater a player battled in one game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerMatchup {
    pub game: GameId,
    pub date: Date,
    pub opponent_team: String,
    pub opponent: Opponent,
    pub battles: Battles,
}

/// Keys ordered by their summed battles, most first; ties keep first appearance.
fn busiest_first<K: Clone + PartialEq>(matchups: &[Matchup], key: impl Fn(&Matchup) -> K) -> Vec<(K, Battles)> {
    let mut totals: Vec<(K, Battles)> = Vec::new();
    for matchup in matchups {
        let k = key(matchup);
        match totals.iter_mut().find(|(existing, _)| *existing == k) {
            Some((_, battles)) => battles.add(Battles::of(matchup)),
            None => totals.push((k, Battles::of(matchup))),
        }
    }
    totals.sort_by_key(|(_, battles)| std::cmp::Reverse(battles.total()));
    totals
}

#[must_use]
pub(crate) fn game_matchups(game: &Game, roster: &HashMap<PlayerId, PlayerRef>) -> GameMatchups {
    arrange(&game.matchups, |id| roster.get(id).cloned())
}

fn arrange(matchups: &[Matchup], player_ref: impl Fn(&PlayerId) -> Option<PlayerRef>) -> GameMatchups {
    let players: Vec<PlayerRef> = busiest_first(matchups, |m| m.player.clone())
        .into_iter()
        .filter_map(|(id, _)| player_ref(&id))
        .collect();
    let opponents: Vec<OpponentBattles> = busiest_first(matchups, |m| m.opponent.clone())
        .into_iter()
        .map(|(opponent, battles)| OpponentBattles { opponent, battles })
        .collect();
    let cells = matchups
        .iter()
        .filter_map(|m| {
            Some(MatchupCell {
                player: players.iter().position(|p| p.id == m.player)?,
                opponent: opponents.iter().position(|o| o.opponent == m.opponent)?,
                battles: Battles::of(m),
            })
        })
        .collect();
    GameMatchups {
        players,
        opponents,
        cells,
    }
}

/// Every opponent `id` battled in `games`, latest game first, then most battles first.
#[must_use]
pub(crate) fn player_matchups(games: &[&Game], id: &PlayerId) -> Vec<PlayerMatchup> {
    let mut rows: Vec<PlayerMatchup> = games
        .iter()
        .flat_map(|game| {
            game.matchups
                .iter()
                .filter(|m| m.player == *id)
                .map(|m| PlayerMatchup {
                    game: game.id.clone(),
                    date: game.date,
                    opponent_team: game.opponent.0.clone(),
                    opponent: m.opponent.clone(),
                    battles: Battles::of(m),
                })
        })
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.battles.total().cmp(&a.battles.total())));
    rows
}

#[cfg(test)]
mod tests {
    use super::{Battles, arrange};
    use crate::analysis::common::PlayerRef;
    use crate::model::{Jersey, Matchup, Opponent, PlayerId, Position};

    fn opponent(surname: &str) -> Opponent {
        Opponent {
            jersey: Some(Jersey(9)),
            surname: surname.to_owned(),
        }
    }

    fn matchup(player: &str, against: &str, won: u32, lost: u32) -> Matchup {
        Matchup {
            player: PlayerId(player.to_owned()),
            opponent: opponent(against),
            battles_won: won,
            battles_lost: lost,
        }
    }

    fn player_ref(id: &PlayerId) -> PlayerRef {
        PlayerRef {
            id: id.clone(),
            name: id.0.clone(),
            jersey: None,
            position: Position::Forward,
        }
    }

    #[test]
    fn busiest_players_and_opponents_come_first_with_cells_indexed_to_them() {
        let matchups = [
            matchup("A", "Stone", 1, 0),
            matchup("B", "Stone", 2, 1),
            matchup("B", "Reed", 0, 4),
        ];

        let arranged = arrange(&matchups, |id| Some(player_ref(id)));

        let players: Vec<&str> = arranged.players.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(players, ["B", "A"]);
        let opponents: Vec<(&str, Battles)> =
            arranged.opponents.iter().map(|o| (o.opponent.surname.as_str(), o.battles)).collect();
        assert_eq!(opponents, [("Stone", Battles { won: 3, lost: 1 }), ("Reed", Battles { won: 0, lost: 4 })]);
        let cell = arranged.cells.iter().find(|c| c.player == 0 && c.opponent == 1).unwrap();
        assert_eq!(cell.battles, Battles { won: 0, lost: 4 });
    }

    #[test]
    fn players_missing_from_the_roster_are_left_out() {
        let matchups = [matchup("A", "Stone", 1, 0), matchup("Gone", "Stone", 1, 1)];

        let arranged = arrange(&matchups, |id| Some(player_ref(id)).filter(|p| p.name != "Gone"));

        assert_eq!(arranged.players.len(), 1);
        assert_eq!(arranged.cells.len(), 1);
    }
}
