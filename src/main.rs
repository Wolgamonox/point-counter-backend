pub mod game_model;
pub mod game_session;

use crate::game_session::{launch_game_session, ClientMessage};

use std::{
    collections::{HashMap, HashSet},
    io,
};

use futures_util::{future, StreamExt, TryStreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock},
};

type GameSessions = Arc<Mutex<HashSet<SocketAddr>>>;

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

    //TODO  Add code here to wait for a message before launching a game session

    let addr = "127.0.0.1:8080";
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Define games
    let game_sessions: GameSessions = Arc::new(Mutex::new(HashSet::new()));

    // Launch a game session
    // tokio::spawn(launch_game_session());

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream));
    }

    Ok(())
}

async fn accept_connection(stream: TcpStream) {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    println!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("New WebSocket connection: {}", addr);

    let (write, read) = ws_stream.split();
    // We should not forward messages other than text or binary.
    read.try_filter(|msg| future::ready(msg.is_text() || msg.is_binary()))
        .forward(write)
        .await
        .expect("Failed to forward messages")
}
