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

/// The function used to deserialize a message.
pub type DeserFn = fn(&[u8]) -> Result<Box<dyn Any + Send + Sync>, io::Error>;
/// The function used to serialize a message.
pub type SerFn = fn(&(dyn Any + Send + Sync)) -> Result<Vec<u8>, io::Error>;

#[derive(Debug)]
/// An enum for the possible states of a connection
pub enum Status {
    /// The connection is still live.
    Connected,
    /// The connection is closed because the peer disconnected by sending a disconnection packet.
    Disconnected(Box<dyn Any + Send + Sync>),
    /// The connection is closed because we chose to close the connection.
    Closed,
    /// The connection was dropped without sending a disconnection packet.
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
    pub fn connected(&self) -> bool {
        match self {
            Status::Connected => true,
            _ => false,
        }
    }

    /// Turns this into an option with the disconnect packet.
    ///
    /// ### Panics
    /// Panics if the generic parameter `D` isn't the disconnect message type (the same `D` that you passed into `MsgTable::build`).
    pub fn disconnected<D: Any + Send + Sync>(&self) -> Option<&D> {
        match self {
            Status::Disconnected(d) => Some(d.downcast_ref().expect("The generic parameter `D` must be the disconnection message type (the same `D` that you passed into `MsgTable::build`).")),
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

/// Message ID.
pub type MId = usize;

/// Connection ID.
pub type CId = u32;

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
}

#[cfg(test)]
mod tests {
    use crate::net::CIdSpec;

    #[test]
    fn cid_spec_all() {
        let spec = CIdSpec::All;

        let cid_vec = vec![0, 1, 2, 3, 10, 20, 1000, 102901];
        let expected_vec = vec![true; cid_vec.len()-1];
        for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
            assert_eq!(spec.matches(cid), expected)
        }
    }

    #[test]
    fn cid_spec_none() {
        let spec = CIdSpec::None;

        let cid_vec = vec![0, 1, 2, 3, 10, 20, 1000, 102901];
        let expected_vec = vec![false; cid_vec.len()-1];
        for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
            assert_eq!(spec.matches(cid), expected)
        }
    }

    #[test]
    fn cid_spec_only() {
        let spec = CIdSpec::Only(12);

        let cid_vec = vec![0, 1, 2, 3, 10, 12, 20, 1000, 102901];
        let mut expected_vec = vec![false; cid_vec.len()-1];
        expected_vec[5] = true;

        for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
            assert_eq!(spec.matches(cid), expected)
        }
    }

    #[test]
    fn cid_spec_except() {
        let spec = CIdSpec::Except(12);

        let cid_vec = vec![0, 1, 2, 3, 10, 12, 20, 1000, 102901];
        let mut expected_vec = vec![true; cid_vec.len()-1];
        expected_vec[5] = false;

        for (cid, expected) in cid_vec.into_iter().zip(expected_vec) {
            assert_eq!(spec.matches(cid), expected)
        }
    }
}
