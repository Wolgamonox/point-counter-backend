use serde::{Deserialize, Serialize};

use std::net::SocketAddr;

use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use crate::game_model::GameState;

/// The maximum number of player in a game
pub const MAX_PLAYER_COUNT: usize = 10;

// Types to communicate between the clients and a game
pub type ClientSender = mpsc::Sender<ClientMessage>;
pub type GameStateReceiver = broadcast::Receiver<GameState>;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    PlayerJoin(String),
    PointEvent {
        player_name: String,
        new_points: i32,
    },
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

    println!("Received a message: {}", &text_msg);

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

pub async fn handle_connection(
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
        let client_tx = client_tx.clone();
        async move { process_client_msg(client_tx, msg).await }
    });

    let incoming_game_state = async_stream::stream! {
        while let Ok(game_state) = game_state_rx.recv().await {

            println!("Got new game state {:?}.", &game_state);
            let json_string = serde_json::to_string(&game_state).expect("Game state should be serializable");
            yield tungstenite::Message::Text(json_string);
        }
    };

    let received_game_state = incoming_game_state.map(Ok).forward(outgoing_client);

    pin_mut!(received_game_state, incoming_client_processed);
    future::select(received_game_state, incoming_client_processed).await;

    println!("{} disconnected", &addr);
}
