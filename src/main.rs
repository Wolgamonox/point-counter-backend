use std::{
    fmt, io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    vec,
};

use anyhow::Error;

use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc},
};

use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt, TryFutureExt};

/// The maximum number of games in the server
const MAX_GAME_COUNT: usize = 16;

/// The maximum number of player in a game
const MAX_PLAYER_COUNT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    name: String,
    points: i32,
}

impl Player {
    fn new(name: String) -> Player {
        Player { name, points: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameState {
    players: Vec<Player>,
    new: bool,
}

impl GameState {
    fn new() -> GameState {
        GameState {
            players: vec![],
            new: true,
        }
    }
}

// Types to communicate between the clients and a game
type ClientSender = mpsc::Sender<PointChange>;
type GameStateReceiver = broadcast::Receiver<GameState>;

#[derive(Debug)]
struct PointChange {
    player_name: String,
    new_points: i32,
}

impl PointChange {
    fn new(player_name: String, new_points: i32) -> PointChange {
        PointChange {
            player_name,
            new_points,
        }
    }
}

async fn handle_connection(
    raw_stream: TcpStream,
    addr: SocketAddr,
    client_tx: ClientSender,
    mut game_state_rx: GameStateReceiver,
) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (outgoing_client, incoming_client) = ws_stream.split();

    // Handle messages incoming from the client
    // if the message is CreateGame or JoinGame, send to the game manager channel
    // else if the messages are related to point change, send to game channel if it exists
    let incoming_client_processed = incoming_client.try_for_each(|msg| {
        let text_msg = msg.to_text().unwrap();
        println!("Received a message: {}", &text_msg);

        let mut msg_list = text_msg.split(":");

        // Test send a point change
        let point_change = PointChange::new(
            msg_list.next().unwrap().to_string(),
            msg_list.next().unwrap().parse().unwrap(),
        );

        // Strange stuff to convert the Send Error of the channel to a tungstenite error
        client_tx.send(point_change).map_err(|e| {
            tungstenite::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
    });

    let incoming_game_state = async_stream::stream! {
        while let Ok(game_state) = game_state_rx.recv().await {

            println!("{:?}", &game_state);
            yield tungstenite::Message::Text(format!("Got new game state: {game_state:?}"));
        }
    };

    let received_game_state = incoming_game_state.map(Ok).forward(outgoing_client);

    pin_mut!(received_game_state, incoming_client_processed);
    future::select(received_game_state, incoming_client_processed).await;

    println!("{} disconnected", &addr);
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    // Add code here to wait for a message before launching a game session

    // Launch a game session
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080";
        // Create the event loop and TCP listener we'll accept connections on.
        let try_socket = TcpListener::bind(&addr).await;
        let listener = try_socket.expect("Failed to bind");
        println!("Listening on: {}", addr);

        // Define game state
        // Temp debug game with one test player
        let game_state = Arc::new(Mutex::new(GameState {
            players: vec![Player::new("Bob".to_string())],
            new: true,
        }));
        // let game_state = Arc::new(Mutex::new(GameState::new()));

        // Define channels to communicate with players
        let (game_state_tx, _game_state_rx) = broadcast::channel::<GameState>(MAX_PLAYER_COUNT);
        let (client_tx, mut client_rx) = mpsc::channel::<PointChange>(MAX_PLAYER_COUNT);

        // Spawn a task that broadcasts a new gamestate whenever there is a point change
        let game_state = Arc::clone(&game_state);
        let session_game_tx = game_state_tx.clone();
        tokio::spawn(async move {
            while let Some(point_change) = client_rx.recv().await {
                // Update game state
                let mut game_state = game_state.lock().unwrap();

                // Change points of player
                (*game_state)
                    .players
                    .iter_mut()
                    .find(|p| p.name == point_change.player_name)
                    .unwrap()
                    .points += point_change.new_points;

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
    });

    loop {}

    Ok(())
}
