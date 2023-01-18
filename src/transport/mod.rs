use crate::net::MsgHeader;
use crate::{MId, MsgTable};
use std::any::Any;
use std::collections::BTreeSet;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};

pub mod client_std_udp;
pub mod server_std_udp;

pub trait ClientTransport {
    fn new(
        local: impl ToSocketAddrs,
        peer: impl ToSocketAddrs,
        msg_table: MsgTable,
    ) -> io::Result<Self>
    where
        Self: Sized;
    fn send(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()>;
    fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)>;
    fn recv_blocking(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

pub trait ServerTransport {
    fn new(
        listen: impl ToSocketAddrs,
        msg_table: MsgTable,
        connected_addrs: Arc<Mutex<BTreeSet<SocketAddr>>>,
    ) -> io::Result<Self>
    where
        Self: Sized;
    fn send_to(&self, addr: SocketAddr, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()>;
    /// Receives the next message (or error) from the transport.
    fn recv_from(&mut self) -> io::Result<(SocketAddr, MsgHeader, Box<dyn Any + Send + Sync>)>;
    /// Returns the address that the server is listening on.
    fn listen_addr(&self) -> io::Result<SocketAddr>;
}
