#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use tokio::runtime::Handle;
use crate::helper::test_packets::{Connection, Disconnect, get_table_parts, Response};

pub mod test_packets;

const ADDR_LOCAL: &str = "0.0.0.0:7788";

pub type Client = carrier_pigeon::Client<Connection, Response, Disconnect>;
pub type Server = carrier_pigeon::Server<Connection, Response, Disconnect>;

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
pub fn create_client_server_pair() -> (Client, Server) {
    let parts = get_table_parts();

    // Create server.
    let mut server = Server::new(ADDR_LOCAL.parse().unwrap(), parts.clone())
        .blocking_recv()
        .unwrap()
        .unwrap();
    let addr = server.listen_addr();
    println!("Server created on addr: {}", addr);

    // Start client connection.
    let client = Client::new(addr, parts, Connection::new("John"));

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    while 0 == server.handle_new_cons(&mut |_con_msg| (true, Response::Accepted)) {}

    // Finish the client connection.
    let (client, response_msg) = client.block().unwrap();

    assert_eq!(response_msg, Response::Accepted);

    (client, server)
}
