//! The client part of the chatroom example.
//!
//! It connects to the server, and allows you to chat with
//! the other people connected.
//!
//! The default address and port is `127.0.0.1:7797`. and
//! the default username is "`MyUser`" however these can
//! be override by running
//! `cargo run --example client <IP ADDRESS AND PORT> <USERNAME>`.
//!
//! You may disconnect from the server by writing
//! `disconnect <REASON>`.
//! You may request the server to disconnect you by writing
//! `disconnect-me`.

use crate::shared::{Connection, Disconnect, Msg, Response, CLIENT_ADDR_LOCAL, SERVER_ADDR_LOCAL};
use carrier_pigeon::net::ClientConfig;
use carrier_pigeon::{Client, Guarantees, MsgTableBuilder};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::io::stdin;
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::Duration;
use std::{env, thread};

mod shared;

fn main() {
    // Create a simple logger
    SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .init()
        .unwrap();

    let mut args = env::args().skip(1);
    // Get the address from the command line args, or use loopback on port 7799.
    let local = args.next().unwrap_or(CLIENT_ADDR_LOCAL.to_owned());
    let peer = args.next().unwrap_or(SERVER_ADDR_LOCAL.to_owned());

    let username = args.next().unwrap_or("MyUser".to_owned());

    // Create the message table.
    // This should be the same on the client and server.
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<Msg>(Guarantees::Unreliable)
        .unwrap();
    let table = builder.build::<Connection, Response, Disconnect>().unwrap();

    let con_msg = Connection {
        user: username.clone(),
    };

    // Start the connection to the server.
    let client = Client::new(local, peer, table, ClientConfig::default(), con_msg);

    // Block until the connection is made.
    let (mut client, resp) = client.block().expect("Failed to connect to server.");

    match resp {
        Response::Accepted => println!("We were accepted!"),
        Response::Rejected(reason) => {
            println!("We were rejected for reason \"{}\"", reason);
            return;
        }
    }

    let receiver = spawn_stdin_thread();

    // This represents the game loop in your favorite game engine.
    loop {
        // If the client is closed, stop running.
        if !client.open() {
            break;
        }
        // These 2 methods should generally be called at the start of every frame.
        // They should also be called before default time so that all other systems get called
        // with the updated messages.

        // This clears the message buffer so that messages from last frame are not carried over.
        client.clear_msgs();
        // Then get the new messages that came in since the last call to this function.
        client.get_msgs();

        // Get messages from the console, and send it to the server.
        while let Ok(text) = receiver.try_recv() {
            if !text.is_empty() {
                const DISCONNECT: &str = "disconnect";
                const DISCONNECT_ME: &str = "disconnect-me";
                if !text.starts_with(DISCONNECT_ME) && text.starts_with(DISCONNECT) {
                    client
                        .disconnect(&Disconnect {
                            reason: text[DISCONNECT.len()..].trim().to_owned(),
                        })
                        .unwrap();
                } else {
                    client
                        .send(&Msg {
                            from: username.clone(),
                            text,
                        })
                        .unwrap();
                }
            }
        }

        if let Some(msg) = client.status().disconnected::<Disconnect>() {
            // Client was disconnected.
            println!("Disconnected for reason {}", msg.reason);
            break;
        }

        // receive messages from the server.
        for msg in client.recv::<Msg>() {
            println!("{}: \"{}\"", msg.from, msg.text);
        }

        // approx 60 Hz
        std::thread::sleep(Duration::from_millis(16));
    }
}

/// Spawns another thread and sends each new line through a channel.
fn spawn_stdin_thread() -> Receiver<String> {
    let (tx, rx) = sync_channel(10);
    thread::spawn(move || {
        let tx = tx; // move

        let mut buff = String::new();
        while let Ok(_n) = stdin().read_line(&mut buff) {
            if tx.send(buff.trim().to_owned()).is_err() {
                return;
            }
            buff.clear();
        }
    });

    rx
}
