//! Networking things that are not specific to either transport.

pub use crate::header::Header;
use std::any::Any;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::Error;

/// The maximum safe message size that can be sent on udp,
/// after taking off the possible overheads from the transport.
///
/// Note that `carrier-pigeon` imposes a 4-byte overhead on every message so
/// the data may be `MAX_SAFE_MESSAGE_SIZE - 4` or less to be guaranteed to be
/// deliverable on udp.
/// [source](https://newbedev.com/what-is-the-largest-safe-udp-packet-size-on-the-internet/)
pub const MAX_SAFE_MESSAGE_SIZE: usize = 508;

/// The absolute maximum packet size that can be received. This is used for
/// sizing the buffer.
///
/// Note that `carrier-pigeon` imposes a 4-byte overhead on every message so
/// the data must be `MAX_MESSAGE_SIZE - 4` or less.
pub const MAX_MESSAGE_SIZE: usize = 2048;

/// An enum representing the 2 possible transports.
///
/// - TCP is reliable but slower.
/// - UDP is un-reliable but quicker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Transport {
    TCP,
    UDP,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetError {
    /// The type was not registered in the [`MsgTable`].
    TypeNotRegistered,
    /// An error occurred in deserialization.
    Deser,
    /// An error occurred in serialization.
    Ser,
    /// The connection was closed.
    Closed,
    /// Tried to preform a Client specific action on a Server type connection.
    NotClient,
    /// Tried to preform a Server specific action on a Client type connection.
    NotServer,
    /// The given CId is not valid for any reason such as: the given CId is not
    /// an active connection
    InvalidCId,
}

impl Display for NetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

/// Message ID
pub type MId = usize;

/// Connection ID
pub type CId = u32;

/// The function used to deserialize a message.
pub type DeserFn = fn(&[u8]) -> Result<Box<dyn Any + Send + Sync>, io::Error>;
/// The function used to serialize a message.
pub type SerFn = fn(&(dyn Any + Send + Sync)) -> Result<Vec<u8>, io::Error>;

#[derive(Debug)]
/// An enum for the possible states of a connection
pub enum Status<D> {
    /// The connection is still live.
    Connected,
    /// The connection is closed because the peer disconnected by sending a disconnection packet.
    Disconnected(D),
    /// The connection is closed because we chose to close the connection.
    Closed,
    /// The connection was dropped without sending a disconnection packet.
    Dropped(Error),
}

impl<D: Display> Display for Status<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "Connected"),
            Self::Disconnected(d) => write!(f, "Disconnected with packet {}", d),
            Self::Closed => write!(f, "Closed"),
            Self::Dropped(e) => write!(f, "Dropped with error {}", e),
        }
    }
}

impl<D> Status<D> {
    pub fn connected(&self) -> bool {
        match self {
            Status::Connected => true,
            _ => false,
        }
    }

    pub fn disconnected(&self) -> Option<&D> {
        match self {
            Status::Disconnected(d) => Some(d),
            _ => None,
        }
    }

    pub fn dropped(&self) -> Option<&Error> {
        match self {
            Status::Dropped(e) => Some(e),
            _ => None,
        }
    }

    pub fn closed(&self) -> bool {
        match self {
            Status::Closed => true,
            _ => false,
        }
    }
}
