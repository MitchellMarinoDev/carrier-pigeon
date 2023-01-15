use crate::net::MsgHeader;
use crate::{MId, MsgTable};
use std::any::Any;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

pub mod std_udp;

pub trait ClientTransport {
    fn new(
        local: impl ToSocketAddrs,
        remote: impl ToSocketAddrs,
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

pub trait ServerTransport {}
