pub mod game_model;
pub mod game_session;

use crate::game_session::{launch_game_session, ClientMessage};

use std::io;

use tokio::{net::TcpListener, sync::mpsc};

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

    // Launch a game session
    tokio::spawn(launch_game_session());

    loop {}

    Ok(())
}
