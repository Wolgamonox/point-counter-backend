#![allow(dead_code)]
#![allow(unused_imports)]

use serde::de::value::Error;
use std::net::TcpListener;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::spawn;
use tungstenite::http::request;
use tungstenite::{accept, Message};

use color_eyre::eyre::{eyre, Result};

use serial_int::SerialGenerator;

use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug)]
struct Player {
    name: String,
    points: i32,
}

impl Player {
    fn new(name: String) -> Player {
        Player { points: 0, name }
    }
}

static ID_GENERATOR: OnceLock<Mutex<SerialGenerator<u32>>> = OnceLock::new();

fn generate_id() -> u32 {
    ID_GENERATOR
        .get_or_init(|| Mutex::new(SerialGenerator::<u32>::new()))
        .lock()
        .expect("A thread panicked while trying to generate an ID.")
        .generate()
}

#[derive(Debug)]
struct Game {
    id: u32,
    players: Vec<Player>,
}

impl Game {
    fn new() -> Game {
        Game {
            players: vec![],
            id: generate_id(),
        }
    }

    fn add_player(&mut self, name: String) -> Result<()> {
        if self.players.iter().any(|p| p.name == name) {
            return Err(eyre!("Player already exists in game."));
        }

        self.players.push(Player::new(name));
        Ok(())
    }

    fn remove_player(&mut self, name: String) -> Result<()> {
        let index = self.players.iter().position(|p| p.name == name);

        match index {
            Some(index) => {
                self.players.remove(index);
                Ok(())
            }
            None => Err(eyre!("This player does not exist in this game.")),
        }
    }

    fn change_points(&mut self, player_name: String, new_points: i32) -> Result<()> {
        let player = self.players.iter_mut().find(|p| p.name == player_name);

        match player {
            Some(player) => {
                player.points = new_points;
                Ok(())
            }
            None => Err(eyre!("This player does not exist in this game.")),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum Request {
    CreateGame {
        player_name: String,
    },
    JoinGame {
        game_id: u32,
        player_name: String,
    },
    PointChange {
        game_id: u32,
        player_name: String,
        new_points: i32,
    },
    RemovePlayer {
        game_id: u32,
        player_name: String,
    },
}

fn process_request(request: Request, games: &mut Vec<Game>) -> Result<()> {
    match request {
        Request::CreateGame { player_name } => {
            let mut new_game = Game::new();
            new_game.add_player(player_name)?;
            games.push(new_game);

            Ok(())
        }
        Request::JoinGame {
            player_name,
            game_id,
        } => {
            let game = games.iter_mut().find(|g| g.id == game_id);

            match game {
                Some(game) => game.add_player(player_name),
                None => Err(eyre!("Game not found.")),
            }
        }
        Request::PointChange {
            game_id,
            player_name,
            new_points,
        } => {
            let game = games.iter_mut().find(|g| g.id == game_id);

            match game {
                Some(game) => game.change_points(player_name, new_points),
                None => Err(eyre!("Game not found.")),
            }
        }
        Request::RemovePlayer {
            game_id,
            player_name,
        } => {
            let game = games.iter_mut().find(|g| g.id == game_id);

            match game {
                Some(game) => game.remove_player(player_name),
                None => Err(eyre!("Game not found.")),
            }
        }
    }
}

fn parse_message(msg: Message) -> Result<Request> {
    let content = msg.clone().into_text()?;

    let msg = serde_json::from_str(content.as_str())?;
    Ok(msg)
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let server = TcpListener::bind("127.0.0.1:9001").unwrap();

    // define state
    let games = Arc::new(Mutex::new(Vec::<Game>::new()));

    for stream in server.incoming() {
        let games = Arc::clone(&games);

        spawn(move || {
            let mut websocket = accept(stream.unwrap()).unwrap();
            loop {
                let read_result = websocket.read();

                // Handles close message
                let msg = match read_result {
                    Ok(msg) => Some(msg),
                    Err(_) => None,
                };

                if msg.is_none() {
                    break;
                }

                let msg = msg.unwrap();

                if msg.is_binary() || msg.is_text() {
                    let mut games = games.lock().unwrap();

                    let result = parse_message(msg)
                        .and_then(|request| process_request(request, &mut *games));


                    // TODO: custom response that are parsable by the app
                    let response = match result {
                        Ok(_) => "OK".to_string(),
                        Err(err) => err.to_string(),
                    };

                    // TODO: handle receiving messages from other channels

                    println!("{:?}", *games);

                    websocket.send(Message::Text(response)).unwrap();
                }
            }
        });
    }

    Ok(())
}
