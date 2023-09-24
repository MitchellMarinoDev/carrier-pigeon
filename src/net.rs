//! Networking things that are not specific to either transport.

use crate::messages::NetMsg;
use serde::{Deserialize, Serialize};
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
pub const MAX_SAFE_MESSAGE_SIZE: usize = 508;

/// The absolute maximum size that udp supports.
///
/// The data must be less than [`MAX_MESSAGE_SIZE`] or it will be dropped.
pub const MAX_MESSAGE_SIZE: usize = 65507;

/// The size of carrier-pigeon's header.
pub const HEADER_SIZE: usize = 12;

/// A header to be sent before the message contents of a message.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct MsgHeader {
    /// The message type of this message.
    pub m_type: MType,
    /// An incrementing integer specific to this `m_type`. This allows us to order messages
    /// on arrival.
    pub order_num: OrderNum,
    /// The [`AckNumber`] of this outgoing message.
    pub sender_ack_num: AckNum,
    /// The header also contains a place to acknowledge the previously received messages that were
    /// from the destination of this message.
    ///
    /// This number is a offset for the `ack_bits`. Read `acknowledgements.md` and look at
    /// [AckSystem](crate::connection::ack_system::AckSystem) for more.
    pub receiver_acking_offset: AckNum,
    /// 32 bits representing weather the 32 ack numbers before the `receiver_acking_num` are acked.
    ///
    /// This allows us to acknowledge up to 32 messages at once.
    ///
    /// For example, with an `receiver_acking_num` of 32
    pub ack_bits: u32,
}

impl MsgHeader {
    /// Creates a [`MsgHeader`] with the given [`MType`], `ack_number` and `order_num`.
    pub fn new(
        m_type: MType,
        order_num: OrderNum,
        sender_ack_num: AckNum,
        receiver_acking_num: AckNum,
        ack_bits: u32,
    ) -> Self {
        MsgHeader {
            m_type,
            order_num,
            sender_ack_num,
            receiver_acking_offset: receiver_acking_num,
            ack_bits,
        }
    }

    /// Converts the [`MsgHeader`] to big endian bytes to be sent over the internet.
    pub fn to_be_bytes(&self) -> [u8; HEADER_SIZE] {
        let m_type_b = (self.m_type as u16).to_be_bytes();
        let order_num_b = self.order_num.to_be_bytes();
        let sender_ack_num_b = self.sender_ack_num.to_be_bytes();
        let receiver_acking_num_b = self.receiver_acking_offset.to_be_bytes();
        let ack_bits_b = self.ack_bits.to_be_bytes();
        debug_assert_eq!(m_type_b.len(), 2);
        debug_assert_eq!(order_num_b.len(), 2);
        debug_assert_eq!(sender_ack_num_b.len(), 2);
        debug_assert_eq!(receiver_acking_num_b.len(), 2);
        debug_assert_eq!(ack_bits_b.len(), 4);
        debug_assert_eq!(
            m_type_b.len()
                + order_num_b.len()
                + sender_ack_num_b.len()
                + receiver_acking_num_b.len()
                + ack_bits_b.len(),
            HEADER_SIZE
        );

        [
            m_type_b[0],
            m_type_b[1],
            order_num_b[0],
            order_num_b[1],
            sender_ack_num_b[0],
            sender_ack_num_b[1],
            receiver_acking_num_b[0],
            receiver_acking_num_b[1],
            ack_bits_b[0],
            ack_bits_b[1],
            ack_bits_b[2],
            ack_bits_b[3],
        ]
    }

    /// Converts the big endian bytes back into a [`MsgHeader`].
    ///
    /// You **must** pass in a slice that is [`HEADER_LEN`] long.
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        assert_eq!(
            bytes.len(),
            HEADER_SIZE,
            "The length of the buffer passed into `from_be_bytes` should have a length of {}",
            HEADER_SIZE
        );

        let m_type = u16::from_be_bytes(bytes[..2].try_into().unwrap()) as usize;
        let order_num = u16::from_be_bytes(bytes[2..4].try_into().unwrap());
        let sender_ack_num = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
        let receiver_acking_num = u16::from_be_bytes(bytes[6..8].try_into().unwrap());
        let ack_bits = u32::from_be_bytes(bytes[8..12].try_into().unwrap());

        MsgHeader {
            m_type,
            order_num,
            sender_ack_num,
            receiver_acking_offset: receiver_acking_num,
            ack_bits,
        }
    }
}

/// Delivery guarantees.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
pub enum Guarantees {
    /// Messages are guaranteed to arrive. Not necessarily in order.
    Reliable,
    /// Messages are guaranteed to arrive in order.
    ReliableOrdered,
    /// The most recent (newest) message is guaranteed to arrive.
    /// You might not get all the messages if they arrive out of order.
    ReliableNewest,
    /// No guarantees about delivery. Messages might or might not arrive.
    Unreliable,
    /// No delivery guarantee, but you will only get the newest message.
    /// If an older message arrives after a newer one, it will be discarded.
    UnreliableNewest,
}

impl Guarantees {
    /// Weather this guarantee is reliable.
    pub fn reliable(&self) -> bool {
        use Guarantees::*;
        match self {
            Reliable | ReliableOrdered | ReliableNewest => true,
            Unreliable | UnreliableNewest => false,
        }
    }

    /// Weather this guarantee is unreliable.
    pub fn unreliable(&self) -> bool {
        !self.reliable()
    }

    /// Weather this guarantee is ordered.
    pub fn ordered(&self) -> bool {
        use Guarantees::*;
        match self {
            ReliableOrdered => true,
            Reliable | ReliableNewest | Unreliable | UnreliableNewest => false,
        }
    }

    /// Weather this guarantee is newest.
    pub fn newest(&self) -> bool {
        use Guarantees::*;
        match self {
            ReliableNewest | UnreliableNewest => true,
            Reliable | ReliableOrdered | Unreliable => false,
        }
    }

    /// Weather this guarantee is ordered or newest.
    pub fn some_ordering(&self) -> bool {
        use Guarantees::*;
        match self {
            ReliableOrdered | ReliableNewest | UnreliableNewest => true,
            Reliable | Unreliable => false,
        }
    }

    /// Weather this guarantee is not ordered or newest.
    pub fn no_ordering(&self) -> bool {
        use Guarantees::*;
        match self {
            Reliable | Unreliable => true,
            ReliableOrdered | ReliableNewest | UnreliableNewest => false,
        }
    }
}

/// An enum for the possible states of a connection.
///
/// Generic types `A`, `R` and `D` are the accept, reject, and disconnect message types (respectively).
pub enum Status<A: NetMsg, R: NetMsg, D: NetMsg> {
    /// Not connected to any peer.
    NotConnected,
    /// Currently connecting to a peer.
    ///
    /// We have sent a connection message, but have yet to hear a response.
    Connecting,
    /// We just got accepted.
    Accepted(A),
    /// We just got rejected.
    Rejected(R),
    /// The connection failed.
    ConnectionFailed(Error),
    /// The connection is established.
    Connected,
    /// The connection is closed because the peer disconnected by sending a disconnection message.
    Disconnected(D),
    /// The connection was dropped without sending a disconnection message.
    Dropped(Error),
    /// Disconnecting from the peer.
    ///
    /// Contains the [`AckNum`] of the sent disconnection message.
    // TODO: add the ack_num of the disconnection message so we can monitor it's ack status.
    Disconnecting(AckNum),
}

impl<A: NetMsg, R: NetMsg, D: NetMsg> Debug for Status<A, R, D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl<A: NetMsg, R: NetMsg, D: NetMsg> Display for Status<A, R, D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::NotConnected => write!(f, "Not connected"),
            Status::Connecting => write!(f, "Connecting..."),
            Status::Accepted(_) => write!(f, "Accepted"),
            Status::Rejected(_) => write!(f, "Rejected"),
            Status::ConnectionFailed(e) => write!(f, "Connection failed with error {}", e),
            Status::Connected => write!(f, "Connected"),
            Status::Disconnected(_) => write!(f, "Disconnected gracefully"),
            Status::Dropped(e) => write!(f, "Dropped with error {}", e),
            Status::Disconnecting(ack_num) => write!(f, "Disconnecting({})...", ack_num),
        }
    }
}

impl<A: NetMsg, R: NetMsg, D: NetMsg> Status<A, R, D> {
    // matches functions

    /// Weather this status is [`NotConnected`](Self::NotConnected).
    pub fn is_not_connected(&self) -> bool {
        matches!(self, Status::NotConnected)
    }

    /// Weather this status is [`Connecting`](Self::Connecting).
    pub fn is_connecting(&self) -> bool {
        matches!(self, Status::Connecting)
    }

    /// Weather this status is [`Accepted`](Self::Accepted).
    pub fn is_accepted(&self) -> bool {
        matches!(self, Status::Accepted(_))
    }

    /// Weather this status is [`Rejected`](Self::Rejected).
    pub fn is_rejected(&self) -> bool {
        matches!(self, Status::Rejected(_))
    }

    /// Weather this status is [`ConnectionFailed`](Self::ConnectionFailed).
    pub fn is_connection_failed(&self) -> bool {
        matches!(self, Status::ConnectionFailed(_))
    }

    /// Weather this status is [`Connected`](Self::Connected).
    pub fn is_connected(&self) -> bool {
        matches!(self, Status::Connected)
    }

    /// Weather this status is [`Disconnected`](Self::Disconnected).
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Status::Disconnected(_))
    }

    /// Weather this status is [`Dropped`](Self::Dropped).
    pub fn is_dropped(&self) -> bool {
        matches!(self, Status::Dropped(_))
    }

    /// Weather this status is [`Disconnecting`](Self::Disconnecting).
    pub fn is_disconnecting(&self) -> bool {
        matches!(self, Status::Disconnecting(_))
    }

    // unwrapping functions

    /// Gets the acceptation message from the [`Accepted`](Self::Accepted) variant.
    /// If this status is not the [`Accepted`](Self::Accepted) variant, this returns `None`.
    pub fn unwrap_accepted(&self) -> Option<&A> {
        match self {
            Status::Accepted(msg) => Some(msg),
            _ => None,
        }
    }

    /// Gets the rejection message from the [`Rejected`](Self::Rejected) variant.
    /// If this status is not the [`Rejected`](Self::Rejected) variant, this returns `None`.
    pub fn unwrap_rejected(&self) -> Option<&R> {
        match self {
            Status::Rejected(msg) => Some(msg),
            _ => None,
        }
    }

    /// Gets the disconnection message from the [`Disconnected`](Self::Disconnected) variant.
    /// If this status is not the [`Disconnected`](Self::Disconnected) variant, this returns `None`.
    pub fn unwrap_disconnected(&self) -> Option<&D> {
        match self {
            Status::Disconnected(msg) => Some(msg),
            _ => None,
        }
    }

    /// Unwraps the connection error from the [`ConnectionFailed`](Self::ConnectionFailed) variant.
    pub fn unwrap_connection_failed(&self) -> Option<&Error> {
        match self {
            Status::ConnectionFailed(err) => Some(err),
            _ => None,
        }
    }

    /// Unwraps the dropped error from the [`Dropped`](Self::Dropped) variant.
    pub fn unwrap_dropped(&self) -> Option<&Error> {
        match self {
            Status::Dropped(err) => Some(err),
            _ => None,
        }
    }
}

/// Message Type.
///
/// This is an integer unique to each type of message.
pub type MType = usize;

/// Connection ID.
///
/// This is an integer incremented for every connection made to the server, so connections can
/// be uniquely identified.
pub type CId = u32;

/// Acknowledgement Number.
///
/// This is an integer incremented for every message sent, so messages can be uniquely identified.
/// This is used as a way to acknowledge reliable messages.
// TODO: this might need to be a wrapper type, as the comparing logic should consider wrapping
// TODO: this should also be a u32 as we might wrap too quickly when sending a lot of messages.
pub type AckNum = u16;

/// Ordering Number.
///
/// This is an integer specific to each [`MType`], incremented for every message sent,
/// This is so we can order the messages as they come in.
// TODO: this might need to be a wrapper type, as the comparing logic should consider wrapping
pub type OrderNum = u16;

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

/// Configuration for a [`Client`](crate::Client) or [`Server`](crate::Server).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct NetConfig {
    /// The minimum number of times to acknowledge a message.
    pub ack_send_count: u32,
    /// The number of sent pings to keep track of. This should be set depending on how often you
    /// are sending pings, and how slow you expect the connection to be.
    pub pings_to_retain: usize,
    /// The number used for smoothing out the RTT. The higher the number, the more smooth/slow
    /// the smoothing is.
    ///
    /// The smoothing is exponential. We move the RTT by (the difference between the current RTT
    /// and the newly estimated RTT)/(`this value`).
    pub ping_smoothing_value: i32,
    /// The interval to try to ping the peer. This allows us to estimate the
    /// RTT witch is needed for reliable system.
    /// This also works as a heartbeat to keep the connection from timing out.
    pub ping_interval: Duration,
    /// The time it takes for a connection to time out. That is, the time it takes for the
    /// connection to close if we haven't heard from the peer.
    ///
    /// Since `carrier-pigeon` sends messages for RTT estimation and for acknowledging other
    /// messages under the hood, you don't need to worry about sending messages to stop the
    /// connection from timing out.
    pub recv_timeout: Duration,
    /// The interval to try to send [`AckMsg`](crate::messages::AckMsg)s the peer. This adds
    /// an extra layer of acknowledging redundancy, keeps the acknowledgement system from falling
    /// behind, and acknowledges old messages that can't get acknowledged otherwise.
    ///
    /// Set this to a duration of 0 to send a message every tick.
    pub ack_msg_interval: Duration,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            ack_send_count: 2,
            pings_to_retain: 8,
            ping_smoothing_value: 8,
            ping_interval: Duration::from_millis(500),
            recv_timeout: Duration::from_secs(10),
            ack_msg_interval: Duration::from_millis(120),
        }
    }
}

/// An untyped network message containing the message content, along with the metadata associated.
pub(crate) struct ErasedNetMsg {
    /// The [`CId`] that the message was sent from.
    ///
    /// On the client side, this will always be `0`.
    pub cid: CId,
    /// The acknowledgment number. This is an incrementing integer assigned by the sender for every
    /// message.
    ///
    /// This is not necessarily guaranteed to be unique as wrapping can happen after a lot of
    /// messages.
    pub ack_num: AckNum,
    /// The ordering number. This is an incrementing integer assigned by the sender, on a
    /// per-[`MType`] basis.
    ///
    /// This is not necessarily guaranteed to be unique as wrapping can happen after a lot of
    /// messages.
    pub order_num: OrderNum,
    /// The actual message.
    pub msg: Box<dyn NetMsg>,
}

impl ErasedNetMsg {
    pub(crate) fn new(
        cid: CId,
        ack_num: AckNum,
        order_num: OrderNum,
        msg: Box<dyn NetMsg>,
    ) -> Self {
        Self {
            cid,
            ack_num,
            order_num,
            msg,
        }
    }

    /// Converts this to NetMsg, borrowed from this.
    pub(crate) fn get_typed<T: NetMsg>(&self) -> Option<Message<T>> {
        let msg = self.msg.downcast_ref()?;
        Some(Message {
            cid: self.cid,
            ack_num: self.ack_num,
            order_num: self.order_num,
            content: msg,
        })
    }
}

/// A network message containing the message content, along with the metadata associated.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct Message<'n, T: NetMsg> {
    /// The [`CId`] that the message was sent from.
    ///
    /// On the client side, this will always be `0`.
    pub cid: CId,
    /// The acknowledgment number. This is an incrementing integer assigned by the sender for every
    /// message.
    ///
    /// This is not necessarily guaranteed to be unique as wrapping can happen after a lot of
    /// messages.
    pub ack_num: AckNum,
    /// The ordering number. This is an incrementing integer assigned by the sender, on a
    /// per-[`MType`] basis.
    ///
    /// This is not necessarily guaranteed to be unique as wrapping can happen after a lot of
    /// messages.
    pub order_num: OrderNum,
    /// The contents of the message that was sent by the peer.
    ///
    /// Borrowed from the client or server.
    pub content: &'n T,
}

impl<'n, T: NetMsg> Deref for Message<'n, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.content
    }
}
