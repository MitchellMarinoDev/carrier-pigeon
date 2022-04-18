#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use std::net::{TcpListener, TcpStream, UdpSocket};
use crate::helper::test_packets::{get_table_parts, Connection, Disconnect, Response};
use log::debug;

pub mod test_packets;

const ADDR_LOCAL: &str = "127.0.0.1:0";

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
    while 0 == server.handle_new_cons(&mut |_con_msg| (true, Response::Accepted)) {}

    // Finish the client connection.
    let (client, response_msg) = client.block().unwrap();
    debug!("Client created on addr: {}", client.local_addr().unwrap());

    assert_eq!(response_msg, Response::Accepted);

    (client, server)
}

/// Creates a pair of [`TcpStream`]s that are connected to each other.
/// Panics if any issues occur.
pub fn create_tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(ADDR_LOCAL).unwrap();
    let addr = listener.local_addr().unwrap();
    let s1 = TcpStream::connect(addr).unwrap();
    let (s2, _) = listener.accept().unwrap();
    (s1, s2)
}

/// Creates a pair of [`UdpSocket`]s that are connected to each other.
/// Panics if any issues occur.
pub fn create_udp_pair() -> (UdpSocket, UdpSocket) {
    let s1 = UdpSocket::bind(ADDR_LOCAL).unwrap();
    let addr1 = s1.local_addr().unwrap();
    let s2 = UdpSocket::bind(ADDR_LOCAL).unwrap();
    let addr2 = s2.local_addr().unwrap();
    s1.connect(addr2).unwrap();
    s2.connect(addr1).unwrap();
    (s1, s2)
}
