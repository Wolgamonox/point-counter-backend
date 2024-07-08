use serde::{Deserialize, Serialize};

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use tungstenite::Message;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        broadcast::{self, Sender},
        mpsc::{self, Receiver},
        Mutex,
    },
    time::Instant,
};

use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use crate::game_model::GameState;

/// The maximum number of player in a game
const MAX_PLAYER_COUNT: usize = 10;

/// The maximum number of games in the server
const MAX_GAME_COUNT: usize = 16;

/// The timeout before a game session is terminated when there are no players
const NO_PLAYER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The interval at which we check that there are no more players
const NO_PLAYER_CHECK_INTERVAL: Duration = Duration::from_secs(30);

// Types to communicate between the clients and a game
type ClientSender = mpsc::Sender<ClientMessage>;
type GameStateReceiver = broadcast::Receiver<GameState>;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    PlayerJoin {
        client_addr: Option<SocketAddr>,
        player_name: String,
    },
    ClientDisconnect {
        client_addr: SocketAddr,
    },
    PointEvent {
        player_name: String,
        new_points: i32,
    },
}

pub async fn launch_game_session(addr: SocketAddr, goal: u32) {
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("[Game server] Listening on: {}", addr);

    // Define game state
    let game_state = Arc::new(Mutex::new(GameState::new(goal)));

    // Define map player client
    let client_player_map = Arc::new(Mutex::new(HashMap::<SocketAddr, String>::new()));

    // Define channels to communicate with players
    let (game_state_tx, _game_state_rx) = broadcast::channel::<GameState>(MAX_GAME_COUNT);
    let (client_tx, client_rx) = mpsc::channel::<ClientMessage>(MAX_PLAYER_COUNT);

    let session_game_tx = game_state_tx.clone();

    // Spawn a task that broadcasts a new gamestate whenever there is a point change
    let game_broadcast_handler = tokio::spawn(game_state_broadcast(
        game_state,
        Arc::clone(&client_player_map),
        client_rx,
        session_game_tx,
    ));

    // Spawn a task for handling player connections
    let connection_handler = tokio::spawn(async move {
        // Spawn a task for each client
        while let Ok((stream, client_addr)) = listener.accept().await {
            // Give the player the channels to communicate
            let client_tx = client_tx.clone();
            let game_state_rx = game_state_tx.subscribe();
            tokio::spawn(handle_connection(
                stream,
                client_addr,
                client_tx,
                game_state_rx,
            ));
        }
    });

    // TODO: terminate game session when no player event for a certain timeout

    // Variables to check if the timeout was reached when there are no players
    let mut timeout_timer_started = false;
    let mut timeout_timer_start = Instant::now();

    loop {
        // Check only every n seconds if the game is empty
        tokio::time::sleep(NO_PLAYER_CHECK_INTERVAL).await;

        // Get client player map
        let client_player_map = client_player_map.lock().await;
        // Verify if there are still players in the game
        if client_player_map.is_empty() && !timeout_timer_started {
            timeout_timer_start = Instant::now();
            timeout_timer_started = true;
        }

        // reset timer to 0 if someone joined in the meantime
        if !client_player_map.is_empty() && timeout_timer_started {
            timeout_timer_started = false;
        }

        // Kill session if the timeout was reached
        if timeout_timer_started && (Instant::now() - timeout_timer_start >= NO_PLAYER_TIMEOUT) {
            println!(
                "[Game server] Timeout completed! Killing game session {}",
                addr.port()
            );
            game_broadcast_handler.abort();
            connection_handler.abort();
            break;
        }
    }
}

async fn game_state_broadcast(
    game_state: Arc<Mutex<GameState>>,
    client_player_map: Arc<Mutex<HashMap<SocketAddr, String>>>,
    mut client_rx: Receiver<ClientMessage>,
    session_game_tx: Sender<GameState>,
) {
    while let Some(client_msg) = client_rx.recv().await {
        // Get game state
        let mut game_state = game_state.lock().await;

        // Get client player map
        let mut client_player_map = client_player_map.lock().await;

        match client_msg {
            ClientMessage::PlayerJoin {
                client_addr,
                player_name,
            } => {
                // Save client addr and name so we can remove it from the game state on disconnect
                (*client_player_map).insert(
                    client_addr
                        .expect("The client address should have been attached.")
                        .clone(),
                    player_name.clone(),
                );
                (*game_state).add_player(player_name);
            }
            ClientMessage::ClientDisconnect { client_addr } => {
                let player_name = (*client_player_map).get(&client_addr);

                match player_name {
                    Some(player_name) => {
                        // client disconnected, remove it from the game state and from the client player map
                        (*game_state).remove_player(player_name.clone());
                        (*client_player_map).remove(&client_addr);
                    }
                    None => (),
                }

                println!("Deleting player");
                println!("Game {:?} Map {:?}", game_state, client_player_map);
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
                    player.set_points(new_points);
                }
            }
        }

        // Send updated game state down channel
        session_game_tx.send(game_state.clone()).unwrap();
    }
}

async fn handle_connection(
    raw_stream: TcpStream,
    client_addr: SocketAddr,
    client_tx: ClientSender,
    mut game_state_rx: GameStateReceiver,
) {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("[Game server] New connection established: {}", client_addr);

    let (outgoing_client, incoming_client) = ws_stream.split();

    // Handle messages incoming from the client
    // if the message is CreateGame or JoinGame, send to the game manager channel
    // else if the messages are related to point change, send to game channel if it exists
    let incoming_client_processed = incoming_client.try_for_each(|msg| {
        let client_tx = client_tx.clone();
        async move { process_client_msg(client_tx, msg, client_addr).await }
    });

    // Process incoming game statess
    let incoming_game_state = async_stream::stream! {
        while let Ok(game_state) = game_state_rx.recv().await {
            let json_string = serde_json::to_string(&game_state).expect("Game state should be serializable");
            yield Message::Text(json_string);
        }
    };

    let received_game_state = incoming_game_state.map(Ok).forward(outgoing_client);

    pin_mut!(received_game_state, incoming_client_processed);
    future::select(received_game_state, incoming_client_processed).await;

    println!("[Game server] {} disconnected", &client_addr);
}

fn new_tunsgenite_error(msg: &str) -> tungstenite::Error {
    tungstenite::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, msg))
}

async fn process_client_msg(
    client_tx: ClientSender,
    msg: Message,
    client_addr: SocketAddr,
) -> Result<(), tungstenite::Error> {
    // Check for disconnection
    let mut text_msg: String = "{}".to_string();

    match msg {
        Message::Close(_) => {
            text_msg =
                serde_json::to_string(&ClientMessage::ClientDisconnect { client_addr }).unwrap();
        }
        Message::Text(text) => {
            text_msg = text;
        }
        _ => (),
    }

    // Check if msg is parseable
    let mut client_msg = match serde_json::from_str(&text_msg) {
        Ok(msg) => msg,
        Err(_) => {
            return Err(new_tunsgenite_error("Could not process message"));
        }
    };

    // If the message is a player join, attach the corresponding client socket address
    if let ClientMessage::PlayerJoin {
        client_addr: _,
        player_name,
    } = &mut client_msg
    {
        client_msg = ClientMessage::PlayerJoin {
            client_addr: Some(client_addr),
            player_name: player_name.clone(),
        };
    }

    // Send message to the channel
    client_tx
        .send(client_msg)
        .await
        .map_err(|_| new_tunsgenite_error("Could not send message to client"))
}
