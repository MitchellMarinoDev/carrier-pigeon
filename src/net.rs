//! Networking things that are not specific to either transport.

use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::{Debug, Display, Formatter};
use std::io::Error;
use std::ops::Deref;
use std::time::Duration;

/// The maximum safe message size that can be sent on udp,
/// after taking off the possible overheads from the transport.
///
/// The data must be [`MAX_SAFE_MESSAGE_SIZE`] or less to be guaranteed to
/// be deliverable on udp.
/// [source](https://newbedev.com/what-is-the-largest-safe-udp-packet-size-on-the-internet/)
pub const MAX_SAFE_MESSAGE_SIZE: usize = 508 - HEADER_SIZE;

/// The absolute maximum size that udp supports.
///
/// The data must be less than [`MAX_MESSAGE_SIZE`] or it will be dropped.
pub const MAX_MESSAGE_SIZE: usize = 65507 - HEADER_SIZE;

/// The size of carrier-pigeon's header.
pub const HEADER_SIZE: usize = 6;

/// A header to be sent before the payload on TCP.
///
/// `len` and `mid` are sent as big endian u16s.
/// This means they have a max value of **`65535`**.
/// This shouldn't pose any real issues.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct MsgHeader {
    /// The message id of this message.
    pub mid: MId,
    /// An incrementing integer unique to this message.
    pub ack_num: u32,
}

impl MsgHeader {
    /// Creates a [`MsgHeader`] with the given [`MId`] and `ack_number`.
    pub fn new(mid: MId, ack_num: u32) -> Self {
        MsgHeader { mid, ack_num }
    }

    /// Converts the [`MsgHeader`] to big endian bytes to be sent over the internet.
    pub fn to_be_bytes(&self) -> [u8; HEADER_SIZE] {
        let mid_b = (self.mid as u16).to_be_bytes();
        let ack_num_b = (self.ack_num as u32).to_be_bytes();

        [
            mid_b[0],
            mid_b[1],
            ack_num_b[0],
            ack_num_b[1],
            ack_num_b[2],
            ack_num_b[3],
        ]
    }

    /// Converts the big endian bytes back into a [`MsgHeader`].
    ///
    /// You **must** pass in a slice that is [`HEADER_LEN`] long.
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), HEADER_SIZE);

        let mid = u16::from_be_bytes(bytes[..2].try_into().unwrap()) as usize;
        let ack_num = u32::from_be_bytes(bytes[2..].try_into().unwrap());

        MsgHeader { mid, ack_num }
    }
}

/// An enum representing the 2 possible transports.
///
/// - TCP is reliable but slower.
/// - UDP is unreliable but quicker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Transport {
    TCP,
    UDP,
}

/// The function used to deserialize a message.
///
/// fn(&[u8]) -> Result<Box<dyn Any + Send + Sync>, io::Error>
pub type DeserFn = fn(&[u8]) -> Result<Box<dyn Any + Send + Sync>, Error>;
/// The function used to serialize a message.
///
/// fn(&(dyn Any + Send + Sync), &mut Vec<u8>) -> Result<(), Error>
pub type SerFn = fn(&(dyn Any + Send + Sync), &mut Vec<u8>) -> Result<(), Error>;

#[derive(Debug)]
/// An enum for the possible states of a connection.
pub enum Status {
    /// The connection is still live.
    Connected,
    /// The connection is closed because the peer disconnected by sending a disconnection message.
    Disconnected(Box<dyn Any + Send + Sync>),
    /// The connection is closed because we chose to close the connection.
    Closed,
    /// The connection was dropped without sending a disconnection message.
    Dropped(Error),
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "Connected"),
            Self::Disconnected(_) => write!(f, "Disconnected gracefully"),
            Self::Closed => write!(f, "Closed"),
            Self::Dropped(e) => write!(f, "Dropped with error {}", e),
        }
    }
}

impl Status {
    /// Returns whether the status is [`Status::Connected`].
    pub fn connected(&self) -> bool {
        matches!(self, Status::Connected)
    }

    /// Turns this into an option with the disconnect message.
    ///
    /// ### Panics
    /// Panics if the generic parameter `D` isn't the disconnect message type (the same `D` that
    /// you passed into `MsgTable::build`).
    pub fn disconnected<D: Any + Send + Sync>(&self) -> Option<&D> {
        match self {
            Status::Disconnected(d) => Some(d.downcast_ref().expect("The generic parameter `D` must be the disconnection message type (the same `D` that you passed into `MsgTable::build`).")),
            _ => None,
        }
    }

    /// Turns this into an option with the disconnect message.
    pub fn disconnected_dyn(&self) -> Option<&Box<dyn Any + Send + Sync>> {
        match self {
            Status::Disconnected(d) => Some(d),
            _ => None,
        }
    }

    /// Turns this into an option with the drop error.
    pub fn dropped(&self) -> Option<&Error> {
        match self {
            Status::Dropped(e) => Some(e),
            _ => None,
        }
    }

    /// Returns whether the status is [`Status::Closed`].
    pub fn closed(&self) -> bool {
        matches!(self, Status::Closed)
    }
}

/// Message ID.
///
/// This is an integer unique to each type of message.
pub type MId = usize;

/// Connection ID.
///
/// This is an integer incremented for every connection made to the server, so connections can
/// be uniquely identified.
pub type CId = u32;

/// Message Number.
///
/// This is an integer incremented for every message sent, so messages can be uniquely identified.
pub type MNum = u32;

/// A way to specify the valid [`CId`]s for an operation.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum CIdSpec {
    /// Matches all [`CId`]s
    All,
    /// Matches no [`CId`]s.
    None,
    /// Matches all except the inner [`CId`]
    Except(CId),
    /// Matches only the inner [`CId`]
    Only(CId),
}

impl CIdSpec {
    /// Weather the given cid matches the pattern.
    pub fn matches(&self, cid: CId) -> bool {
        match self {
            CIdSpec::All => true,
            CIdSpec::None => false,
            CIdSpec::Except(o) => cid != *o,
            CIdSpec::Only(o) => cid == *o,
        }
    }

    /// Checks if the `other` [`CIdSpec`] overlaps (shares at least on common [`CId`]).
    pub fn overlaps(&self, other: CIdSpec) -> bool {
        use CIdSpec::*;

        match (*self, other) {
            (None, _) => false,
            (_, None) => false,
            (All, _) => true,
            (_, All) => true,

            (Except(_), Except(_)) => true,
            (Only(o1), Only(o2)) => o1 == o2,

            (Only(only), Except(except)) => only != except,
            (Except(except), Only(only)) => only != except,
        }
    }
}

/// Configuration for a client or server.
///
/// This needs to be defined before starting up the server.
///
/// Contains all configurable information about the server.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct NetConfig {
    /// The timeout for handling new connections. The time to wait for a connection message
    /// after establishing a tcp connection.
    pub timeout: Duration,
    /// The maximum number of connections that this can handle at the same time.
    pub max_con_handle: usize,
    /// The maximum message size in bytes. This is used for sizing the buffer for TCP and UDP.
    /// Any attempts to send messages over this size will be discarded. Keep in mind, there is
    /// still a soft limit for UDP messages (`MAX_SAFE_MSG_SIZE`)
    pub max_msg_size: usize,
}

impl NetConfig {
    /// Creates a new Server configuration.
    pub fn new(timeout: Duration, max_con_handle: usize, max_msg_size: usize) -> Self {
        NetConfig {
            timeout,
            max_con_handle,
            max_msg_size,
        }
    }
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            timeout: Duration::from_millis(5_000),
            max_con_handle: 4,
            max_msg_size: 2048,
        }
    }
}

/// An untyped network message containing the message content, along with the metadata associated.
#[derive(Debug)]
pub(crate) struct ErasedNetMsg {
    /// The [`CId`] that the message was sent from.
    pub(crate) cid: CId,
    /// The message.
    pub(crate) msg: Box<dyn Any + Send + Sync>,
}

impl ErasedNetMsg {
    pub(crate) fn new(cid: CId, msg: Box<dyn Any + Send + Sync>) -> Self {
        Self { cid, msg }
    }

    /// Converts this to NetMsg, borrowed from this.
    pub(crate) fn get_typed<T: Any + Send + Sync>(&self) -> Option<NetMsg<T>> {
        let msg = self.msg.downcast_ref()?;
        Some(NetMsg {
            cid: self.cid,
            m: msg,
        })
    }
}

/// A network message containing the message content, along with the metadata associated.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct NetMsg<'n, T: Any + Send + Sync> {
    /// The [`CId`] that the message was sent from.
    pub cid: CId,
    /// The actual message.
    ///
    /// Borrowed from the client or server.
    pub m: &'n T,
}

impl<'n, T: Any + Send + Sync> Deref for NetMsg<'n, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.m
    }
}
