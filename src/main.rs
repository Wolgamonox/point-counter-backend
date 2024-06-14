pub mod game_model;
pub mod game_session;

use crate::game_model::GameState;
use crate::game_session::{handle_connection, ClientMessage, MAX_PLAYER_COUNT};

use std::{
    io,
    sync::{Arc, Mutex},
};

use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};

/// The maximum number of games in the server
const MAX_GAME_COUNT: usize = 16;

async fn launch_game_session() {
    let addr = "127.0.0.1:8080";
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Define game state
    let game_state = Arc::new(Mutex::new(GameState::new()));

    // Define channels to communicate with players
    let (game_state_tx, _game_state_rx) = broadcast::channel::<GameState>(MAX_GAME_COUNT);
    let (client_tx, mut client_rx) = mpsc::channel::<ClientMessage>(MAX_PLAYER_COUNT);

    let game_state = Arc::clone(&game_state);
    let session_game_tx = game_state_tx.clone();

    // Spawn a task that broadcasts a new gamestate whenever there is a point change
    tokio::spawn(async move {
        while let Some(client_msg) = client_rx.recv().await {
            // Get game state
            let mut game_state = game_state.lock().unwrap();

            match client_msg {
                ClientMessage::PlayerJoin(player_name) => {
                    (*game_state).add_player(player_name);
                }
                ClientMessage::PointEvent {
                    player_name,
                    new_points,
                } => {
                    // Change points of player
                    let player = (*game_state)
                        .players
                        .iter_mut()
                        .find(|p| p.name == player_name);

                    if let Some(player) = player {
                        player.add_points(new_points);
                    }
                }
            }

            // Send updated game state down channel
            session_game_tx.send(game_state.clone()).unwrap();
        }
    });

    // Spawn a task for each client
    while let Ok((stream, addr)) = listener.accept().await {
        // Give the player the channels to communicate
        let client_tx = client_tx.clone();
        let game_state_rx = game_state_tx.subscribe();
        tokio::spawn(handle_connection(stream, addr, client_tx, game_state_rx));
    }
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    // Test
    println!(
        "{}",
        serde_json::to_string(&ClientMessage::PlayerJoin("Gaston".to_string())).unwrap()
    );
    println!(
        "{}",
        serde_json::to_string(&ClientMessage::PointEvent {
            player_name: "Gaston".to_string(),
            new_points: 0
        })
        .unwrap()
    );

    // Add code here to wait for a message before launching a game session

    // Launch a game session
    tokio::spawn(launch_game_session());

    loop {}

    Ok(())
}
