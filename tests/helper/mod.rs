#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use crate::helper::test_messages::{get_msg_table, Accepted, Connection, Disconnect, Rejected};
use carrier_pigeon::net::NetConfig;
use carrier_pigeon::Response;
use log::{debug, info};
use std::thread::sleep;
use std::time::Duration;

pub mod test_messages;

type Client = carrier_pigeon::Client<Connection, Accepted, Rejected, Disconnect>;
type Server = carrier_pigeon::Server<Connection, Accepted, Rejected, Disconnect>;

pub const SERVER_ADDR_LOCAL: &str = "127.0.0.1:7778";
pub const CLIENT_ADDR_LOCAL: &str = "127.0.0.1:7776";

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
pub fn create_client_server_pair() -> (Client, Server) {
    let msg_table = get_msg_table();

    debug!("Creating server.");
    let mut server = Server::new(
        NetConfig::default(),
        SERVER_ADDR_LOCAL.parse().unwrap(),
        msg_table.clone(),
    )
    .unwrap();
    debug!("Server created!");
    let addr = server.listen_addr().unwrap();
    debug!("Server listening on addr: {}", addr);

    debug!("Creating client.");
    // Start client connection.
    let mut client = Client::new(NetConfig::default(), msg_table);
    debug!("Client Connecting");
    client.connect(
        CLIENT_ADDR_LOCAL.parse().unwrap(),
        SERVER_ADDR_LOCAL.parse().unwrap(),
        &Connection::new("John Smith"),
    );

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    loop {
        client.tick();
        server.tick();
        let count = server.handle_pending(|_cid, _addr, _con_msg: Connection| {
            Response::Accepted::<Accepted, Rejected>(Accepted)
        });
        if count != 0 {
            break;
        }
        sleep(Duration::from_millis(1));
    }

    // Block until the connection is made.
    let mut status = client.get_status();
    while status.is_connecting() {
        client.tick();
        status = client.get_status();
        sleep(Duration::from_millis(1));
    }

    assert!(status.is_accepted(), "{}", status);
    let status = client.handle_status();

    debug!("Client created on addr: {}", client.local_addr().unwrap());

    assert_eq!(status.unwrap_accepted(), Some(&Accepted));

    (client, server)
}
