use std::{collections::HashMap, io, net::SocketAddr};

use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc, oneshot},
};

use futures_util::{pin_mut, StreamExt, TryStreamExt};
use tungstenite::Message;

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
}

impl GameState {
    fn new(player_name: String) -> GameState {
        GameState {
            players: vec![Player::new(player_name)],
        }
    }
}

// Types to communicate between the clients and the game manager
type ManagerSender = mpsc::Sender<Command>;
type CommandSender = oneshot::Sender<GameHandle>;

// Types to communicate between the clients and a game
type ClientSender = mpsc::Sender<PointChange>;
type GameStateReceiver = broadcast::Receiver<GameState>;

#[derive(Debug)]
struct PointChange {
    player_name: String,
    new_points: i32,
}

#[derive(Debug)]
enum Command {
    CreateGame {
        player_name: String,
        game_channel_resp: CommandSender,
    },
}

/// Structure for the manager to keep the channel accesses to pass to clients
#[derive(Debug, Clone)]
struct GameHandle {
    client_sender: ClientSender,
    game_state_receiver: GameStateReceiver,
}

/// Process incoming message from the client
async fn process_incoming(
    msg: Message,
    manager_tx: &ManagerSender,
) -> Result<(), tungstenite::Error> {
    println!("Received a message: {}", msg.to_text().unwrap());

    if msg.to_text().unwrap() == "CreateGame".to_string() {
        println!("Asking game manager to create a game...");

        // Define a oneshot channel so the manager can send us back a GameHandle
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::CreateGame {
            player_name: "TestPlayer".to_string(),
            game_channel_resp: resp_tx,
        };

        // Send the command
        manager_tx.send(cmd).await.unwrap();

        // Await the response
        let res = resp_rx.await;
        println!("GOT = {:?}", res);
    }



    Ok(())
}

async fn handle_connection(raw_stream: TcpStream, manager_tx: ManagerSender, addr: SocketAddr) {
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
        let mut games: HashMap<u32, GameHandle> = HashMap::new();

        while let Some(cmd) = manager_rx.recv().await {
            println!("Game manager received cmd {:?}", cmd);
            match cmd {
                Command::CreateGame {
                    player_name,
                    game_channel_resp,
                } => {
                    // TODO handle errors

                    // New game channel
                    let (game_state_tx, mut game_state_rx) = broadcast::channel(MAX_PLAYER_COUNT);
                    let (client_tx, mut client_rx) = mpsc::channel(MAX_PLAYER_COUNT);

                    // Store new game
                    let game_handle = GameHandle {
                        client_sender: client_tx,
                        game_state_receiver: game_state_rx,
                    };
                    games.insert(0, game_handle);

                    // Spawn new game thread
                    tokio::spawn(async move {
                        let game = GameState::new(player_name);
                    });

                    let _ = game_channel_resp.send(game_handle.clone());
                }
            }
        }
    });

    // Spawn a thread for each client
    while let Ok((stream, addr)) = listener.accept().await {
        // Give the player a sender to the manager
        tokio::spawn(handle_connection(stream, manager_tx.clone(), addr));
    }

    Ok(())
}
