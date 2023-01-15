use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use crate::MId;

pub mod std_udp;

pub trait ClientTransport {
    fn new(local: impl ToSocketAddrs, remote: impl ToSocketAddrs) -> io::Result<Self> where Self: Sized;
    fn send(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

pub trait ServerTransport {}
