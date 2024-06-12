#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

use std::{
    clone,
    collections::HashMap,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use color_eyre::eyre::{eyre, Context, Result};
use serial_int::SerialGenerator;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};

#[derive(Debug, Clone)]
struct Player {
    name: String,
    points: i32,
}

impl Player {
    fn new(name: String) -> Player {
        Player { name, points: 0 }
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

#[derive(Debug, Clone)]
struct Game {
    players: Vec<Player>,
}

impl Game {
    fn new() -> Game {
        Game { players: vec![] }
    }

    fn with_player(player: Player) -> Game {
        Game {
            players: vec![player],
        }
    }
}

#[derive(Debug)]
enum PlayerEvent {
    CreateGame,
    JoinGame {
        game_id: u32,
        player_name: String,
    },
    PointChange {
        game_id: u32,
        player_name: String,
        new_points: i32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let (game_update_tx, game_update_rx) = broadcast::channel::<Game>(16);

    let (player_chan_tx_original, mut player_chan_rx) = mpsc::channel::<PlayerEvent>(100);

    // Start thread of game manager
    tokio::spawn(async move {
        let mut games: HashMap<u32, Game> = HashMap::new();

        //TODO proper error handlings

        // Handle incoming events
        while let Some(event) = player_chan_rx.recv().await {
            println!("got = {:?}", &event);

            match event {
                PlayerEvent::CreateGame => {
                    games.insert(generate_id(), Game::new());
                }
                PlayerEvent::JoinGame {
                    game_id,
                    player_name,
                } => {
                    //TODO check max amount of players
                    let game = games.get_mut(&game_id).unwrap();
                    game.players.push(Player::new(player_name));
                }
                PlayerEvent::PointChange {
                    game_id,
                    player_name,
                    new_points,
                } => {
                    let game = games.get_mut(&game_id).unwrap();
                    let player = game
                        .players
                        .iter_mut()
                        .find(|p| p.name == player_name)
                        .unwrap();

                    player.points = new_points;
                }
            }

            println!("Games: {games:?}");

            // TODO: Notify all players in the game
        }
    });

    // Spawn a thread for each player connection
    for i in 0..3 {
        println!("New player connected");
        let mut game_update_rx = game_update_tx.subscribe();
        let player_chan_tx = player_chan_tx_original.clone();

        // Thread 1 handles the receiving of game updates
        tokio::spawn(async move {
            while let Ok(game) = game_update_rx.recv().await {
                println!("Receiver {i} got: {game:?}");
            }
        });

        // Thread 2 handles the sending of player events
        tokio::spawn(async move {
            player_chan_tx.send(PlayerEvent::CreateGame).await.unwrap();
        });
    }

    // game_update_tx.send(Game::new())?;
    // game_update_tx.send(Game::new())?;

    // let player_chan_tx_new = player_chan_tx.clone();

    // player_chan_tx_new.send(PlayerEvent::CreateGame).await?;
    // time::sleep(Duration::from_secs(1)).await;
    // player_chan_tx_new
    //     .send(PlayerEvent::JoinGame {
    //         game_id: 0,
    //         player_name: "Gaston".to_string(),
    //     })
    //     .await?;
    // time::sleep(Duration::from_secs(1)).await;
    // player_chan_tx_new
    //     .send(PlayerEvent::PointChange {
    //         game_id: 0,
    //         player_name: "Gaston".to_string(),
    //         new_points: 15,
    //     })
    //     .await?;
    // time::sleep(Duration::from_secs(1)).await;

    // player_chan_tx
    //     .send(PlayerEvent::JoinGame {
    //         game_id: 0,
    //         player_name: "Theo".to_string(),
    //     })
    //     .await?;
    // time::sleep(Duration::from_secs(1)).await;
    // player_chan_tx
    //     .send(PlayerEvent::PointChange {
    //         game_id: 0,
    //         player_name: "Theo".to_string(),
    //         new_points: 15,
    //     })
    //     .await?;
    // time::sleep(Duration::from_secs(1)).await;

    // Leave time for program to receive everything
    time::sleep(Duration::from_secs(5)).await;

    Ok(())
}
