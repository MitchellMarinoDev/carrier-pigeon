use std::io;
use std::net::{SocketAddr, UdpSocket};
use crate::net::MAX_MESSAGE_SIZE;
use crate::transport::Transport;

pub struct UdpTransport {
    socket: UdpSocket,
    buf: [u8; MAX_MESSAGE_SIZE],
}

impl Transport for UdpTransport {
    fn new_client(local: SocketAddr, peer: SocketAddr) -> io::Result<Self> where Self: Sized {
        let socket = UdpSocket::bind(local)?;
        socket.connect(peer)?;

        Ok(UdpTransport {
            socket,
            buf: [0; MAX_MESSAGE_SIZE],
        })
    }

    fn new_server(listen: SocketAddr) -> io::Result<Self> where Self: Sized {
        let socket = UdpSocket::bind(listen)?;

        Ok(UdpTransport {
            socket,
            buf: [0; MAX_MESSAGE_SIZE],
        })
    }

    fn recv(&mut self) -> io::Result<(SocketAddr, Vec<u8>)> {
        todo!()
    }

    fn recv_blocking(&mut self) -> io::Result<(SocketAddr, Vec<u8>)> {
        todo!()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        todo!()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        todo!()
    }
}