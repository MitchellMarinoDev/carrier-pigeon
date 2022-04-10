#![allow(unused)]
//! Helper functions and types to make setting up the tests easier.

use std::net::{TcpListener, TcpStream, UdpSocket};

const ADDR_LOCAL: &str = "127.0.0.1:0";

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