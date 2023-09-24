//! The server part of the chatroom example.
//!
//! This is a non interactive server, so you may not send
//! messages with this.
//!
//! The default address and port is `127.0.0.1:7777`, however
//! these can be override by running
//! `cargo run --example server <IP ADDRESS AND PORT>`.
//!
//! If a client sends a message saying `disconnect-me`,
//! they will be disconnected, and their message will
//! not be broadcast to the other clients.

use crate::shared::{Accepted, Connection, Disconnect, Msg, Rejected, SERVER_ADDR_LOCAL};
use carrier_pigeon::net::{CIdSpec, NetConfig};
use carrier_pigeon::{Guarantees, MsgTableBuilder, Response, Server};
use std::env;
use std::time::Duration;

mod shared;

fn main() {
    let _ = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init();

    let mut args = env::args().skip(1);
    // Get the address from the command line args, or use loopback on port 7777.
    let listen = args
        .next()
        .unwrap_or(SERVER_ADDR_LOCAL.to_owned())
        .parse()
        .expect("failed to parse the first arg as a SocketAddr");

    // Create the message table.
    // This should be the same on the client and server.
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<Msg>(Guarantees::ReliableOrdered)
        .unwrap();

    let table = builder
        .build::<Connection, Accepted, Rejected, Disconnect>()
        .unwrap();

    // Start the server.
    let mut server =
        Server::new(NetConfig::default(), listen, table).expect("failed to create server");

    let blacklisted_users = vec!["john", "jane"];

    // This represents the game loop in your favorite game engine.
    loop {
        server.tick();

        // This should be called every once in a while to clean up so that the
        // server doesn't send messages to disconnected clients.
        while let Some(disconnect_event) = server.handle_disconnect() {
            println!(
                "CId {} disconnected: {:?}",
                disconnect_event.cid, disconnect_event.disconnection_type
            );
        }

        // This handles the new connections with whatever logic you want.
        server.handle_pending(|_cid, _addr, con_msg: Connection| {
            // You can capture variables from the context to decide if you want
            // to accept or reject the connection request.
            let blacklisted = blacklisted_users.contains(&&*con_msg.user.to_lowercase());

            if blacklisted {
                Response::Rejected(Rejected {
                    reason: "This user is blacklisted".to_owned(),
                })
            } else {
                Response::Accepted(Accepted)
            }
        });

        let mut cids_to_disconnect = vec![];

        let mut msgs = vec![];
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
            msgs.push((msg.cid, msg.content.clone()));
        }
        for (cid, msg) in msgs {
            server.send_spec(CIdSpec::Except(cid), &msg).unwrap();
        }

        for cid in cids_to_disconnect {
            server
                .disconnect(
                    Disconnect {
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
