#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use crate::helper::test_packets::{get_table_parts, Connection, Disconnect, Response};
use log::debug;

pub mod test_packets;

pub const ADDR_LOCAL: &str = "127.0.0.1:0";

pub type Client = carrier_pigeon::Client<Connection, Response, Disconnect>;
pub type Server = carrier_pigeon::Server<Connection, Response, Disconnect>;

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
pub fn create_client_server_pair() -> (Client, Server) {
    let parts = get_table_parts();

    debug!("Creating server.");
    let mut server = Server::new(ADDR_LOCAL.parse().unwrap(), parts.clone()).unwrap();
    let addr = server.listen_addr();
    debug!("Server created on addr: {}", addr);

    debug!("Creating client.");
    // Start client connection.
    let client = Client::new(addr, parts, Connection::new("John"));

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    while 0 == server.handle_new_cons(&mut |_cid, _con_msg| (true, Response::Accepted)) {}

    // Finish the client connection.
    let (client, response_msg) = client.block().unwrap();
    debug!("Client created on addr: {}", client.local_addr().unwrap());

    assert_eq!(response_msg, Response::Accepted);

    (client, server)
}
