pub mod game_model;
pub mod game_session;

use crate::game_session::{launch_game_session, ClientMessage};

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::OnceLock,
};

use futures_util::{SinkExt, StreamExt};
use serial_int::SerialGenerator;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

// TODO: add obfuscation of ports with sqids crate
type Port = u16;

static ID_GENERATOR: OnceLock<Mutex<SerialGenerator<Port>>> = OnceLock::new();

async fn generate_id() -> Port {
    // Generate id starting from 1
    ID_GENERATOR
        .get_or_init(|| Mutex::new(SerialGenerator::<Port>::with_init_value(1)))
        .lock()
        .await
        .generate()
}

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

async fn accept_connection(stream: TcpStream) {
    let addr = stream
        .peer_addr()
        .expect("Connected streams should have a peer address");

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("[Main Server] New connection: {}", addr);

    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(msg) => {
                let text_msg = msg.to_text().unwrap();

                if text_msg == "CreateGame".to_string() {
                    let port = BASE_PORT + generate_id().await;
                    let addr = SocketAddr::new(BASE_ADDR, port);

                    tokio::spawn(launch_game_session(addr));

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
