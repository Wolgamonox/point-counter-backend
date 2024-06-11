use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread::spawn;
use tungstenite::{accept, Message};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct JoinGameMessage {

}

#[derive(Debug)]
struct Player {
    points: i32,
}

impl Player {
    fn new() -> Player {
        Player { points: 0}
    }
}

#[derive(Debug)]
struct Game {
    players: Vec<Player>,
}

impl Game {
    fn new() -> Game {
        Game { players: vec![] }
    }
}

fn main() {
    let server = TcpListener::bind("127.0.0.1:9001").unwrap();

    // define state
    let counter = Arc::new(Mutex::new(0));

    for stream in server.incoming() {
        let counter = Arc::clone(&counter);

        spawn(move || {
            let mut websocket = accept(stream.unwrap()).unwrap();
            loop {
                let msg = websocket.read().unwrap();

                println!("new msg");
                // We do not want to send back ping/pong messages.
                if msg.is_binary() || msg.is_text() {
                    let content = msg.clone().into_text().unwrap();

                    let num_to_add = content.parse::<i32>().unwrap_or(0);

                    let mut num = counter.lock().unwrap();

                    *num += num_to_add;

                    websocket.send(Message::Text((*num).to_string())).unwrap();
                }
            }
        });
    }
}
