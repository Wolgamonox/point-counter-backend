#![allow(dead_code)]
#![allow(unused_imports)]

use std::{clone, thread};

use color_eyre::eyre::{eyre, Context, Result};
use tokio::sync::broadcast;

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
    id: u32,
    players: Vec<Player>,
}

impl Game {
    fn new() -> Game {
        Game {
            players: vec![],
            id: 0,
        }
    }
}

#[derive(Clone)]
enum Event {
    GameUpdate,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // define state
    // let games = Arc::new(Mutex::new(Vec::<Game>::new()));

    let (tx, mut rx1) = broadcast::channel::<Event>(16);
    let mut rx2 = tx.subscribe();

    // mimics one connection to the server
    tokio::spawn(async move {
        println!("{}", "hey");
    });

    Ok(())
}
