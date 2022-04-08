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

use crate::shared::{Connection, Disconnect, Msg, Response};
use carrier_pigeon::{MsgTable, Server, Transport};
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
    let addr = args.next().unwrap_or("127.0.0.1:7797".to_owned());
    let addr = addr.parse().expect("Could not parse address.");

    // Create the message table.
    // This should be the same on the client and server.
    let mut table = MsgTable::new();
    table.register::<Msg>(Transport::UDP).unwrap();

    let parts = table.build::<Connection, Response, Disconnect>().unwrap();

    // Start the server.
    let server = Server::new(addr, parts);

    // Block until the server is finished being created.
    let mut server = server
        .expect("Failed to create server.");

    let blacklisted_users = vec!["John", "Jane"];

    // This represents the game loop in your favorite game engine.
    loop {
        // These 2 methods should generally be called at the start of every frame.
        // This clears the message buffer so that messages from last frame are not carried over.
        server.clear_msgs();
        // Then get the new messages that came in since the last call to this function.
        server.recv_msgs();

        // This should be called every once in a while to clean up so that the
        // server doesn't send packets to disconnected clients.
        server.handle_disconnects(
            &mut |cid, status| {
                println!("CId {} disconnected with status: {:?}", cid, status);
            },
        );

        // This handles the new connections with whatever logic you want.
        server.handle_new_cons(&mut |con_msg| {
            // You can capture variables from the context to decide if you want
            // to accept or reject the connection request.
            let blacklisted = blacklisted_users.contains(&&*con_msg.user);

            if blacklisted {
                let resp_msg = Response::Rejected("This user is blacklisted".to_owned());
                (false, resp_msg)
            } else {
                let resp_msg = Response::Accepted;
                (true, resp_msg)
            }
        });

        let mut cids_to_disconnect = vec![];

        let msgs = server.recv::<Msg>().unwrap()
            .map(|(cid, msg)| (cid, msg.clone()))
            .collect::<Vec<_>>();
        for (cid, msg) in msgs {
            println!(
                "Client {} sent message: {}: \"{}\"",
                cid, msg.from, msg.text
            );

            // If the client sent the message of "disconnect-me", disconnect them.
            if msg.text == "disconnect-me" {
                cids_to_disconnect.push(cid);
                continue;
            }

            // Broadcast the message to all other clients.
            server.broadcast_except(&msg, cid).unwrap();
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
