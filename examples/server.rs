//! The server part of the chatroom example.
//!
//! This is a non interactive server, so you may not send
//! messages with this.
//!
//! The default address and port is `127.0.0.1:7797`, however
//! these can be override by running
//! `cargo run --example server <IP ADDRESS AND PORT>`.
//!
//! If a client sends a message saying `disconnect-me`,
//! they will be disconnected, and their message will
//! not be broadcast to the other clients.

use crate::shared::{Connection, Disconnect, Msg, Response, CLIENT_ADDR_LOCAL, SERVER_ADDR_LOCAL};
use carrier_pigeon::net::{CIdSpec, NetConfig};
use carrier_pigeon::{Guarantees, MsgTable, MsgTableBuilder, Server, Transport};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::env;
use std::time::Duration;

mod shared;

fn main() {
    // Create a simple logger
    SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .init()
        .unwrap();

    let mut args = env::args().skip(1);
    // Get the address from the command line args, or use loopback on port 7799.
    let listen = args.next().unwrap_or(SERVER_ADDR_LOCAL.to_owned());

    // Create the message table.
    // This should be the same on the client and server.
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<Msg>(Guarantees::Unreliable)
        .unwrap();

    let table = builder.build::<Connection, Response, Disconnect>().unwrap();

    // Start the server.
    let mut server =
        Server::new(listen, table, NetConfig::default()).expect("Failed to create server.");

    let blacklisted_users = vec!["john", "jane"];

    // This represents the game loop in your favorite game engine.
    loop {
        // These 2 methods should generally be called at the start of every frame.
        // This clears the message buffer so that messages from last frame are not carried over.
        server.clear_msgs();
        // Then get the new messages that came in since the last call to this function.
        server.get_msgs();

        // This should be called every once in a while to clean up so that the
        // server doesn't send messages to disconnected clients.
        server.handle_disconnects(|cid, status| {
            println!("CId {} disconnected with status: {:?}", cid, status);
        });

        // This handles the new connections with whatever logic you want.
        server.handle_new_cons(|_cid, _addr, con_msg: Connection| {
            // You can capture variables from the context to decide if you want
            // to accept or reject the connection request.
            let blacklisted = blacklisted_users.contains(&&*con_msg.user.to_lowercase());

            if blacklisted {
                let resp_msg = Response::Rejected("This user is blacklisted".to_owned());
                (false, resp_msg)
            } else {
                let resp_msg = Response::Accepted;
                (true, resp_msg)
            }
        });

        let mut cids_to_disconnect = vec![];

        for msg in server.recv::<Msg>() {
            println!(
                "Client {} sent message: {}: \"{}\"",
                msg.cid, msg.from, msg.text
            );

            // If the client sent the message of "disconnect-me", disconnect them.
            if msg.text == "disconnect-me" {
                cids_to_disconnect.push(msg.cid);
                continue;
            }

            // Broadcast the message to all other clients.
            server.send_spec(CIdSpec::Except(msg.cid), msg.m).unwrap();
        }

        for cid in cids_to_disconnect {
            server
                .disconnect(
                    &Disconnect {
                        reason: "Disconnect requested.".to_string(),
                    },
                    cid,
                )
                .unwrap();
        }

        // approx 60 Hz
        std::thread::sleep(Duration::from_millis(16));
    }
}
