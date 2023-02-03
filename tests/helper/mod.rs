#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use crate::helper::test_messages::{get_msg_table, Connection, Disconnect, Response};
use carrier_pigeon::net::{ClientConfig, ServerConfig};
use carrier_pigeon::{Client, Server};
use log::debug;

pub mod test_messages;

pub const SERVER_ADDR_LOCAL: &str = "127.0.0.1:7778";
pub const CLIENT_ADDR_LOCAL: &str = "127.0.0.1:7776";

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
pub fn create_client_server_pair() -> (Client, Server) {
    let msg_table = get_msg_table();

    debug!("Creating server.");
    let mut server = Server::new(
        SERVER_ADDR_LOCAL.parse().unwrap(),
        msg_table.clone(),
        ServerConfig::default(),
    )
    .unwrap();
    debug!("Server created!");
    let addr = server.listen_addr().unwrap();
    debug!("Server listening on addr: {}", addr);

    debug!("Creating client.");
    // Start client connection.
    let client = Client::new(
        CLIENT_ADDR_LOCAL.parse().unwrap(),
        SERVER_ADDR_LOCAL.parse().unwrap(),
        msg_table,
        ClientConfig::default(),
        Connection::new("John"),
    );

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    loop {
        server.tick();
        let count =
            server.handle_new_cons(|_cid, _addr, _con_msg: Connection| (true, Response::Accepted));
        if count != 0 {
            break;
        }
    }

    // Finish the client connection.
    let (client, response_msg) = client.block::<Response>().unwrap();
    debug!("Client created on addr: {}", client.local_addr().unwrap());

    assert_eq!(response_msg, Response::Accepted);

    (client, server)
}
