#![allow(unused)]
//! Test messages for use in tests.

use carrier_pigeon::{Guarantees, MsgTable, MsgTableBuilder};
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A reliable test message.
pub struct ReliableMsg {
    pub msg: String,
}
impl ReliableMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        ReliableMsg { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// An unreliable test message.
pub struct UnreliableMsg {
    pub msg: String,
}
impl UnreliableMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        UnreliableMsg { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test connection message.
pub struct Connection {
    pub usr: String,
}
impl Connection {
    pub fn new<A: Into<String>>(usr: A) -> Self {
        Connection { usr: usr.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test disconnection message.
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

/// The accepted message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Accepted;

/// The rejected message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Rejected {
    pub reason: String,
}


/// Builds a table with all these test messages and returns it's parts.
pub fn get_msg_table() -> MsgTable<Connection, Accepted, Rejected, Disconnect> {
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    builder
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();
    builder.build::<Connection, Accepted, Rejected, Disconnect>().unwrap()
}
