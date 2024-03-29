#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use crate::helper::test_messages::{get_table_parts, Connection, Disconnect, Response};
use carrier_pigeon::net::Config;
use carrier_pigeon::tcp::TcpCon;
use carrier_pigeon::{Client, Server};
use log::debug;
use std::net::{TcpListener, TcpStream, UdpSocket};

pub mod test_messages;

const ADDR_LOCAL: &str = "127.0.0.1:0";

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
pub fn create_client_server_pair() -> (Client, Server) {
    let parts = get_table_parts();

    debug!("Creating server.");
    let mut server = Server::new(
        ADDR_LOCAL.parse().unwrap(),
        parts.clone(),
        Config::default(),
    )
    .unwrap();
    let addr = server.listen_addr();
    debug!("Server created on addr: {}", addr);

    debug!("Creating client.");
    // Start client connection.
    let client = Client::new(addr, parts, Config::default(), Connection::new("John"));

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    while 0 == server.handle_new_cons(|_cid, _con_msg: Connection| (true, Response::Accepted)) {}

    // Finish the client connection.
    let (client, response_msg) = client.block::<Response>().unwrap();
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

/// Creates a pair of [`TcpCon`]s that are connected to each other.
/// Panics if any issues occur.
pub fn create_tcp_con_pair() -> (TcpCon, TcpCon) {
    let (s1, s2) = create_tcp_pair();
    let s1 = TcpCon::from_stream(s1, 2048);
    let mut s2 = TcpCon::from_stream(s2, 2048);
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
