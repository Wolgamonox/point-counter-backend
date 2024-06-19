pub mod game_model;
pub mod game_session;

use crate::game_session::{launch_game_session, ClientMessage};

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    ops::Not,
};

use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

// TODO: add obfuscation of ports with sqids crate
type Port = u16;

const BASE_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
const BASE_PORT: Port = 9000;

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    // Test
    println!(
        "{}",
        serde_json::to_string(&ClientMessage::PlayerJoin {
            client_addr: None,
            player_name: "Gaston".to_string()
        })
        .unwrap()
    );
    println!(
        "{}",
        serde_json::to_string(&ClientMessage::PointEvent {
            player_name: "Gaston".to_string(),
            new_points: 0
        })
        .unwrap()
    );

    let server_addr = SocketAddr::new(BASE_ADDR, BASE_PORT);

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&server_addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("[Main server] Listening on: {}", server_addr);

    // Launch a game session
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream));
    }

    Ok(())
}

struct GameSession(Port, JoinHandle<()>);

async fn accept_connection(stream: TcpStream) {
    let addr = stream
        .peer_addr()
        .expect("Connected streams should have a peer address");

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("[Main Server] New connection: {}", addr);

    let (mut write, mut read) = ws_stream.split();

    let mut game_sessions: Vec<GameSession> = Vec::new();
    let mut available_ports: Vec<Port> = (9001..9021 as Port).collect();

    while let Some(msg) = read.next().await {
        // Verify if game sessions are finished to see if we can free some ports
        game_sessions = game_sessions
            .into_iter()
            .filter(|session| {
                if session.1.is_finished() {
                    // add back the port to the available ports
                    available_ports.push(session.0);
                }
                // keep only sessions that are not finished
                session.1.is_finished().not()
            })
            .collect();

        match msg {
            Ok(msg) => {
                let text_msg = msg.to_text().unwrap();

                if text_msg == "CreateGame".to_string() {
                    let port = available_ports.pop();

                    let Some(port) = port else {
                        // No game server available
                        write
                            .send(tungstenite::Message::Text("null".to_string()))
                            .await
                            .expect("Failed to send response");
                        break;
                    };

                    let addr = SocketAddr::new(BASE_ADDR, port);

                    let session_handle = tokio::spawn(launch_game_session(addr));

                    game_sessions.push(GameSession(port, session_handle));

                    println!("[Main server] Creating a game hosted on {port}");

                    // Send back port to the client so it can connect to the game session
                    write
                        .send(tungstenite::Message::Text(port.to_string()))
                        .await
                        .expect("Failed to send response");
                }
            }
            Err(e) => {
                println!("[Main server] error: {e}");
            }
        }
    }
}
