pub mod game_model;
pub mod game_session;

use crate::game_session::{launch_game_session, ClientMessage};

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    ops::Not,
    sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
};

// TODO: add obfuscation of ports with sqids crate
type Port = u16;

const BASE_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
const BASE_PORT: Port = 9000;

struct GameSession {
    port: Port,
    handle: JoinHandle<()>,
}

impl GameSession {
    fn new(port: Port, handle: JoinHandle<()>) -> GameSession {
        GameSession { port, handle }
    }
}

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

    let game_sessions = Arc::new(Mutex::new(Vec::<GameSession>::new()));
    let available_ports = Arc::new(Mutex::new(Vec::from_iter(9001..9021 as Port)));

    // Launch a game session
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(
            stream,
            Arc::clone(&game_sessions),
            Arc::clone(&available_ports),
        ));
    }

    Ok(())
}

async fn accept_connection(
    stream: TcpStream,
    game_sessions: Arc<Mutex<Vec<GameSession>>>,
    available_ports: Arc<Mutex<Vec<Port>>>,
) {
    let client_addr: SocketAddr = stream
        .peer_addr()
        .expect("Connected streams should have a peer address");

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("[Main Server] New connection: {}", client_addr);

    let (mut write, mut read) = ws_stream.split();

    // get the game sessions and available ports
    let mut game_sessions = game_sessions.lock().await;
    let mut available_ports = available_ports.lock().await;

    while let Some(msg) = read.next().await {
        // Verify if game sessions are finished to see if we can free some ports
        game_sessions.retain(|session| {
            if session.handle.is_finished() {
                // add back the port to the available ports
                available_ports.push(session.port);
            }
            // keep only sessions that are not finished
            session.handle.is_finished().not()
        });

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

                    game_sessions.push(GameSession::new(port, session_handle));

                    println!("[Main server] Creating a game hosted on {port}");
                    println!("Available ports: {available_ports:?}");

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
    println!("[Main server] Disconnected: {}", client_addr);
}
