//! Code that is shared between the client and server examples.
//!
//! This is mostly just the Message type declarations.

use carrier_pigeon::NetMsg;
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, NetMsg)]
/// A packet for a message from a user
pub struct Msg {
    pub from: String,
    pub text: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, NetMsg)]
/// The connection packet.
pub struct Connection {
    pub user: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, NetMsg)]
/// The disconnection packet.
pub struct Disconnect {
    pub reason: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, NetMsg)]
/// The response packet.
pub enum Response {
    Accepted,
    Rejected(String),
}
