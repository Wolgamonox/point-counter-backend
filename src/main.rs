use std::{io, net::SocketAddr};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

use futures_util::{pin_mut, StreamExt, TryStreamExt};
use tungstenite::Message;

#[derive(Debug)]
struct Player {
    name: String,
    points: i32,
}

impl Player {
    fn new(name: String) -> Player {
        Player { name, points: 0 }
    }
}

#[derive(Debug)]
struct Game {
    players: Vec<Player>,
}

impl Game {
    fn new(player_name: String) -> Game {
        Game {
            players: vec![Player::new(player_name)],
        }
    }
}

type GameChannelSender = mpsc::Sender<Game>;
type CommandSender = oneshot::Sender<GameChannelSender>;
type ManagerChannelSender = mpsc::Sender<Command>;

#[derive(Debug)]
enum Command {
    CreateGame {
        player_name: String,
        game_channel_resp: CommandSender,
    },
}

async fn process_incoming(
    msg: Message,
    manager_tx: &ManagerChannelSender,
) -> Result<(), tungstenite::Error> {
    println!("Received a message: {}", msg.to_text().unwrap());

    if msg.to_text().unwrap() == "CreateGame".to_string() {
        println!("Asking game manager to create a game...");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::CreateGame {
            player_name: "TestPlayer".to_string(),
            game_channel_resp: resp_tx,
        };

        // Send the GET request
        manager_tx.send(cmd).await.unwrap();

        // Await the response
        let res = resp_rx.await;
        println!("GOT = {:?}", res);
    }

    Ok(())
}

async fn handle_connection(
    raw_stream: TcpStream,
    manager_tx: ManagerChannelSender,
    addr: SocketAddr,
) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (outgoing, incoming) = ws_stream.split();

    // Handle messages incoming from the client
    // if the message is CreateGame or JoinGame, send to the game manager channel
    // else if the messages are related to point change, send to game channel if it exists
    let incoming_processed = incoming.try_for_each(|msg| process_incoming(msg, &manager_tx));

    // let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(incoming_processed);
    incoming_processed.await.unwrap();

    println!("{} disconnected", &addr);
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let addr = "127.0.0.1:8080";

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Game manager channel
    let (manager_tx, mut manager_rx) = mpsc::channel(16);

    // Spawn game manager thread
    tokio::spawn(async move {
        // TODO record games in a hash map here
        while let Some(cmd) = manager_rx.recv().await {
            println!("Game manager received cmd {:?}", cmd);
            match cmd {
                Command::CreateGame {
                    player_name,
                    game_channel_resp,
                } => {
                    // TODO handle errors

                    // New game channel
                    let (game_channel_tx, mut game_channel_rx) = mpsc::channel(10);

                    // Spawn new game thread
                    tokio::spawn(async move {
                        let game = Game::new(player_name);
                    });

                    let _ = game_channel_resp.send(game_channel_tx);
                }
            }
        }
    });

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        let new_manager_tx = manager_tx.clone();
        tokio::spawn(handle_connection(stream, new_manager_tx, addr));
    }

    Ok(())
}
