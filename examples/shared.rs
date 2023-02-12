//! Code that is shared between the client and server examples.
//!
//! This is mostly just the Message type declarations.

use serde::{Deserialize, Serialize};

pub const CLIENT_ADDR_LOCAL: &str = "127.0.0.1:7776";
pub const SERVER_ADDR_LOCAL: &str = "127.0.0.1:7777";

/// A message from a user
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Msg {
    pub from: String,
    pub text: String,
}

/// The connection message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Connection {
    pub user: String,
}

/// The disconnection message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Disconnect {
    pub reason: String,
}

/// The accepted message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Accepted;

/// The rejected message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Rejected {
    pub reason: String,
}
