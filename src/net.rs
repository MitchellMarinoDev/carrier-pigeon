//! Networking things that are not specific to either transport.

use std::any::Any;
use crate::net::TaskStatus::{Done, Failed, Running};
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::Error;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;

/// The maximum safe packet size that can be sent on udp,
/// after taking off the possible overheads from the transport.
///
/// Note that `carrier-pigeon` imposes a 4 byte overhead on every message.
/// This overhead ***is*** accounted for in this const.
pub const MAX_SAFE_PACKET_SIZE: usize = 504;

/// The absolute maximum packet size that can be received.
/// This is used for sizing the buffer.
pub const MAX_PACKET_SIZE: usize = 1024;

/// A header to be sent before the actual contents of the packet.
///
/// `len` and `mid` are sent as u16s.
/// This means they have a max value of **`65535`**.
/// This shouldn't pose any real issues.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub(crate) struct Header {
    /// The message id.
    pub mid: MId,
    /// Then length of the packet without the header.
    pub len: usize,
}

impl Header {
    /// Creates a [`Header`] with the given [`MId`] and `length`.
    pub(crate) fn new(mid: MId, len: usize) -> Self {
        Header { mid, len }
    }

    /// Converts the [`Header`] to big endian bytes to be sent over
    /// the internet.
    pub(crate) fn to_be_bytes(&self) -> [u8; 4] {
        let mid_b = (self.mid as u16).to_be_bytes();
        let len_b = (self.len as u16).to_be_bytes();

        [mid_b[0], mid_b[1], len_b[0], len_b[1]]
    }

    /// Converts the big endian bytes back into a [`Header`].
    pub(crate) fn from_be_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), 4);

        let mid = u16::from_be_bytes(bytes[..2].try_into().unwrap()) as usize;
        let len = u16::from_be_bytes(bytes[2..].try_into().unwrap()) as usize;

        Header { mid, len }
    }
}

#[cfg(test)]
mod tests {
    use crate::net::Header;

    #[test]
    fn to_from_bytes() {
        let points = vec![
            (0, 0),
            (2, 2),
            (100, 34),
            (65530, 982),
        ];

        for point in points {
            let header = Header::new(point.0, point.1);
            let ser = header.to_be_bytes();
            let de = Header::from_be_bytes(&ser);
            assert_eq!(header, de);
        }
    }
}


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

pub enum Resp<R, C> {
    Accepted(R, C),
    Rejected(R),
}

/// A type for keeping track of the result of a task
/// that might or might not be finished.
///
/// note: before using this type, call [`update()`](TaskStatus::update)
/// on it to get its updated value.
pub enum TaskStatus<T> {
    /// The task finished.
    Done(T),
    /// The task is still running.
    Running(oneshot::Receiver<T>),
    /// The sender dropped before sending a value.
    Failed,
}

impl<T> TaskStatus<T> {
    /// Creates a new [`TaskStatus`] with the given receiver.
    pub fn new(channel: oneshot::Receiver<T>) -> Self {
        Running(channel)
    }

    /// Gets the updated value of the [`TaskStatus`]. This should
    /// be called everytime before using the value.
    pub fn update(&mut self) {
        // Take the value out so that we can work with it.
        let mut tmp = std::mem::replace(self, Failed);

        if let Running(mut status) = tmp {
            tmp = match status.try_recv() {
                Ok(done) => Done(done),
                Err(TryRecvError::Empty) => Running(status),
                Err(TryRecvError::Closed) => Failed,
            };
        }

        // put the value back in
        *self = tmp;
    }

    /// If the value is [Done], returns
    /// [`Some(val)`](Some), otherwise [`None`].
    pub fn done(&self) -> Option<&T> {
        match self {
            Self::Done(d) => Some(d),
            _ => None,
        }
    }

    /// Returns whether the task is still running.
    pub fn is_running(&self) -> bool {
        match self {
            Self::Running(_) => true,
            _ => false,
        }
    }

    /// Returns whether the task is failed.
    pub fn is_failed(&self) -> bool {
        match self {
            Self::Failed => true,
            _ => false,
        }
    }
}

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

    pub fn dropped(&self) -> Option<&Error>{
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
