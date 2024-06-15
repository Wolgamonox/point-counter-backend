use std::ops::Not;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    points: i32,
}

impl Player {
    pub fn new(name: String) -> Player {
        Player { name, points: 0 }
    }

    pub fn add_points(self: &mut Self, points: i32) {
        self.points += points;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<Player>,
    pub new: bool,
}

impl GameState {
    pub fn new() -> GameState {
        GameState {
            players: vec![],
            new: true,
        }
    }

    pub fn add_player(self: &mut Self, name: String) {
        if self.players.iter().any(|p| p.name == name).not() {
            self.players.push(Player::new(name));
        }
    }

    pub fn remove_player(self: &mut Self, name: String) {
        self.players
            .iter()
            .position(|p| p.name == name)
            .inspect(|i| {
                self.players.remove(*i);
            });
    }
}
