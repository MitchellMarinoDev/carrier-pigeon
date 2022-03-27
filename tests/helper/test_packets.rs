#![allow(unused)]
//! Test packets for use in tests.

use carrier_pigeon::{MsgTable, MsgTableParts, NetMsg, Transport};
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// A test packet for TCP.
pub struct TcpPacket {
    pub msg: String,
}
impl TcpPacket {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        TcpPacket { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// A test packet for UDP.
pub struct UdpPacket {
    pub msg: String,
}
impl UdpPacket {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        UdpPacket { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// A test connection packet.
pub struct Connection {
    pub usr: String,
}
impl Connection {
    pub fn new<A: Into<String>>(usr: A) -> Self {
        Connection { usr: usr.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// A test disconnection packet.
pub struct Disconnect {
    pub reason: String,
}
impl Disconnect {
    pub fn new<A: Into<String>>(reason: A) -> Self {
        Disconnect {
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// A test response packet.
pub enum Response {
    Accepted,
    Rejected(String),
}
impl Response {
    pub fn rejected<A: Into<String>>(reason: A) -> Self {
        Response::Rejected(reason.into())
    }
    pub fn accepted() -> Self {
        Response::Accepted
    }
}

/// Builds a table with all these test packets and returns it's parts.
pub fn get_table_parts() -> MsgTableParts<Connection, Response, Disconnect> {
    let mut table = MsgTable::new();
    table.register::<TcpPacket>(Transport::TCP).unwrap();
    table.register::<UdpPacket>(Transport::UDP).unwrap();
    table.build().unwrap()
}
