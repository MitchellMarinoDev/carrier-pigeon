use crate::MType;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

pub mod client_std_udp;
pub mod server_std_udp;
mod std_udp;

// TODO: combine transports.
pub trait ClientTransport {
    /// Create a new instance of this transport, with the local address `local`
    /// connecting to the peer address `peer`.
    ///
    /// A [`ClientTransport`] is an implementation of a one-to-one networking transport.
    fn new(local: SocketAddr, peer: SocketAddr) -> io::Result<Self>
    where
        Self: Sized;
    /// Send the payload `payload` to the connected address.
    ///
    /// The [`MType`](crate::MType) of the message is not needed, but provided for logging purposes.
    fn send(&self, m_type: MType, payload: Arc<Vec<u8>>) -> io::Result<()>;
    /// Receive the next message (or error) from the connected address.
    fn recv(&mut self) -> io::Result<Vec<u8>>;
    fn recv_blocking(&mut self) -> io::Result<Vec<u8>>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

pub trait ServerTransport {
    /// Create a new instance of this transport, listening on `listen`.
    ///
    /// A [`ServerTransport`] is an implementation of a one-to-many networking transport.
    fn new(listen: SocketAddr) -> io::Result<Self>
    where
        Self: Sized;
    /// Sends the payload `payload` to the address `addr`.
    ///
    /// The [`MType`](crate::MType) of the message is not needed, but provided for logging purposes.
    fn send_to(&self, addr: SocketAddr, m_type: MType, payload: Arc<Vec<u8>>) -> io::Result<()>;
    /// Receives the next message (or error) from from any address/client.
    fn recv_from(&mut self) -> io::Result<(SocketAddr, Vec<u8>)>;
    /// Returns the address that the server is listening on.
    fn listen_addr(&self) -> io::Result<SocketAddr>;
}

pub trait Transport {
    fn new_client(local: SocketAddr, peer: SocketAddr) -> io::Result<Self>
    where
        Self: Sized;
    fn new_server(listen: SocketAddr) -> io::Result<Self>
    where
        Self: Sized;
    fn recv(&mut self) -> io::Result<(SocketAddr, Vec<u8>)>;
    // TODO: combine this with the client constructor.
    fn recv_blocking(&mut self) -> io::Result<(SocketAddr, Vec<u8>)>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}
