use serde::{Deserialize, Serialize};

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc},
};

use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use crate::game_model::GameState;

/// The maximum number of player in a game
const MAX_PLAYER_COUNT: usize = 10;

/// The maximum number of games in the server
const MAX_GAME_COUNT: usize = 16;

// Types to communicate between the clients and a game
type ClientSender = mpsc::Sender<ClientMessage>;
type GameStateReceiver = broadcast::Receiver<GameState>;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    PlayerJoin(String),
    PointEvent {
        player_name: String,
        new_points: i32,
    },
}

pub async fn launch_game_session(addr: SocketAddr) {
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("[Game server] Listening on: {}", addr);

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

async fn process_client_msg(
    client_tx: ClientSender,
    msg: tungstenite::Message,
) -> Result<(), tungstenite::Error> {
    // Check for disconnection
    let text_msg = match msg.to_text() {
        Ok(text) => text,
        Err(_) => {
            return Err(tungstenite::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Disconnected",
            )))
        }
    };

    // Check if msg is parseable
    let client_msg = match serde_json::from_str(text_msg) {
        Ok(msg) => msg,
        Err(_) => {
            return Err(tungstenite::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Could not process message",
            )))
        }
    };

    match client_tx.send(client_msg).await {
        Ok(_) => Ok(()),
        Err(e) => Err(tungstenite::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))),
    }
}

async fn handle_connection(
    raw_stream: TcpStream,
    addr: SocketAddr,
    client_tx: ClientSender,
    mut game_state_rx: GameStateReceiver,
) {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("[Game server] New connection established: {}", addr);

    let (outgoing_client, incoming_client) = ws_stream.split();

    // Handle messages incoming from the client
    // if the message is CreateGame or JoinGame, send to the game manager channel
    // else if the messages are related to point change, send to game channel if it exists
    let incoming_client_processed = incoming_client.try_for_each(|msg| {
        let client_tx = client_tx.clone();
        async move { process_client_msg(client_tx, msg).await }
    });

    // Process incoming game statess
    let incoming_game_state = async_stream::stream! {
        while let Ok(game_state) = game_state_rx.recv().await {
            let json_string = serde_json::to_string(&game_state).expect("Game state should be serializable");
            yield tungstenite::Message::Text(json_string);
        }
    };

    let received_game_state = incoming_game_state.map(Ok).forward(outgoing_client);

    pin_mut!(received_game_state, incoming_client_processed);
    future::select(received_game_state, incoming_client_processed).await;

    println!("[Game server] {} disconnected", &addr);
    // TODO have a map of who is connected and which address corresponds to which player
}
