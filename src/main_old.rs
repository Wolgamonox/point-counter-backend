// #![allow(dead_code)]
// #![allow(unused_imports)]

use std::net::TcpListener;
use std::rc::Weak;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::spawn;
use tungstenite::{accept, Message};

use color_eyre::eyre::{eyre, Context, Result};

use serial_int::SerialGenerator;

use serde::{Deserialize, Serialize};
use serde_json::{self, json, to_string};

struct Observer {}

#[derive(Debug, Serialize, Deserialize)]
struct Player {
    name: String,
    points: i32,

    #[serde(skip)]
    game: Option<Arc<Game>>,
}

impl Player {
    fn new(name: String) -> Player {
        Player {
            name,
            points: 0,
            game: None,
        }
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

#[derive(Debug, Serialize, Deserialize)]
struct Game {
    id: u32,
    players: Vec<Weak<Player>>,
}

impl Game {
    fn new() -> Game {
        Game {
            players: vec![],
            id: generate_id(),
        }
    }

    // fn add_player(&mut self, name: String) -> Result<()> {
    //     if self.players.iter().any(|p| p.name == name) {
    //         return Err(eyre!("Player already exists in game."));
    //     }

    //     self.players.push(Player::new(name));
    //     Ok(())
    // }

    // fn remove_player(&mut self, name: String) -> Result<()> {
    //     let index = self.players.iter().position(|p| p.name == name);

    //     match index {
    //         Some(index) => {
    //             self.players.remove(index);
    //             Ok(())
    //         }
    //         None => Err(eyre!("This player does not exist in this game.")),
    //     }
    // }

    // fn change_points(&mut self, player_name: String, new_points: i32) -> Result<()> {
    //     let player = self.players.iter_mut().find(|p| p.name == player_name);

    //     match player {
    //         Some(player) => {
    //             player.points = new_points;
    //             Ok(())
    //         }
    //         None => Err(eyre!("This player does not exist in this game.")),
    //     }
    // }
}

#[derive(Serialize, Deserialize)]
enum Request {
    // CreateGame,
    // JoinGame {
    //     game_id: u32,
    //     player_name: String,
    // },
    // PointChange {
    //     game_id: u32,
    //     player_name: String,
    //     new_points: i32,
    // },
    // RemovePlayer {
    //     game_id: u32,
    //     player_name: String,
    // },
    CreatePlayer { player_name: String },
}

#[derive(Serialize, Deserialize)]
enum Response {
    Ok,
    Game(Game),
    Error(String),
}

fn process_request(
    request: Request,
    games: &mut Vec<Game>,
    player: &Player,
) -> Result<Option<Player>> {
    match request {
        // Request::CreateGame => {
        //     let mut new_game = Game::new();
        //     new_game.add_player(player)?;

        //     let response = to_string(&new_game).expect("Game should be parsable");

        //     games.push(new_game);
        //     Ok(None)
        // }
        // Request::JoinGame {
        //     player_name,
        //     game_id,
        // } => {
        //     let game = games.iter_mut().find(|g| g.id == game_id);

        //     match game {
        //         Some(game) => {
        //             game.add_player(player_name)?;
        //             let response = to_string(game).expect("Game should be parsable");
        //             Ok(response)
        //         }
        //         None => Err(eyre!("Game not found.")),
        //     }
        // }
        // Request::PointChange {
        //     game_id,
        //     player_name,
        //     new_points,
        // } => {
        //     let game = games.iter_mut().find(|g| g.id == game_id);

        //     match game {
        //         Some(game) => {
        //             game.change_points(player_name, new_points)?;
        //             let response = to_string(game).expect("Game should be parsable");
        //             Ok(response)
        //         }
        //         None => Err(eyre!("Game not found.")),
        //     }
        // }
        // Request::RemovePlayer {
        //     game_id,
        //     player_name,
        // } => {
        //     let game = games.iter_mut().find(|g| g.id == game_id);

        //     match game {
        //         Some(game) => {
        //             game.remove_player(player_name)?;
        //             let response = to_string(game).expect("Game should be parsable");
        //             Ok(response)
        //         }
        //         None => Err(eyre!("Game not found.")),
        //     }
        // }
        Request::CreatePlayer { player_name } => Ok(Some(Player::new(player_name))),
    }
}

fn parse_message(msg: Message) -> Result<Request> {
    let content = msg.clone().into_text()?;

    let msg = serde_json::from_str(content.as_str())
        .wrap_err_with(|| format!("Failed to parse request: {}", content))?;
    Ok(msg)
}

fn main() -> Result<()> {
    color_eyre::install()?;

    println!(
        "{}",
        to_string(&Request::CreatePlayer {
            player_name: "Gaston".to_string()
        })
        .unwrap()
    );

    let server = TcpListener::bind("127.0.0.1:9001").unwrap();

    // define state
    let games = Arc::new(Mutex::new(Vec::<Game>::new()));

    for stream in server.incoming() {
        let games = Arc::clone(&games);

        spawn(move || {
            let mut websocket = accept(stream.unwrap()).unwrap();

            // player object for this connection
            // name will be provided by the app throught a message
            let mut player = Player::new(String::new());

            loop {
                let Ok(msg) = websocket.read() else {
                    // Break on connection close
                    break;
                };

                if msg.is_binary() || msg.is_text() {
                    // only access when searching for a game
                    let mut games = games.lock().unwrap();

                    let result = parse_message(msg).and_then(|r| process_request(r, &mut *games));

                    let response = match result {
                        Ok(Some(new_player)) => {
                            player = new_player;
                            Response::Ok
                        }
                        Ok(None) => Response::Ok,
                        Err(err) => Response::Error(json!({"error": err.to_string()}).to_string()),
                    };

                    // TODO: handle receiving messages from other channels

                    println!("{:?}", player);
                    println!("{:?}", *games);

                    // convert response to string
                    let response = to_string(&response).expect("Response should be parsable");

                    websocket.send(Message::Text(response)).unwrap();
                }
            }
        });
    }

    Ok(())
}

// Examples:

// println!(
//     "{}",
//     to_string(&Request::CreateGame {
//         player_name: "Gaston".to_string()
//     })
//     .unwrap()
// );

// println!(
//     "{}",
//     to_string(&Request::JoinGame {
//         game_id: 0,
//         player_name: "Theo".to_string()
//     })
//     .unwrap()
// );

// println!(
//     "{}",
//     to_string(&Request::PointChange {
//         game_id: 0,
//         player_name: "Gaston".to_string(),
//         new_points: 10,
//     })
//     .unwrap()
// );

// {"CreatePlayer":{"player_name":"Gaston"}}
// {"CreateGame":{"player_name":"Gaston"}}
// {"JoinGame":{"game_id":0,"player_name":"Theo"}}
// {"PointChange":{"game_id":0,"player_name":"Gaston","new_points":10}}
