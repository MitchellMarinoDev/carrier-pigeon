use crate::connection::ack_system::AckSystem;
use crate::connection::ordering_system::OrderingSystem;
use crate::messages::{AckMsg, NetMsg};
use crate::net::{AckNum, MsgHeader};
use crate::{Guarantees, MType, MsgTable, NetConfig};
use std::time::{Duration, Instant};

/// A minimum time for ack messages to be sent.
/// [`AckMsg`]s are sent once per frame, limited by this number.
const ACK_MSG_LIMIT: Option<Duration> = Some(Duration::from_millis(100));

/// A system that handles the reliablility and ordering of incoming messages based on their
/// [`Guarantees`].
///
/// Generic parameter `SD` is for "Send Data". It should be the data that you send to the
/// transport other than the header.
///
/// Generic parameter `RD` is for "Receive Data". It should be the data that you get from the
/// transport other than the header.
///
/// Since these differ between client and server (server needs to keep track of a from address),
/// these need to be generic parameters.
pub(crate) struct ReliableSystem<SD: Clone, RD, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    msg_table: MsgTable<C, A, R, D>,
    last_ack_msg: Instant,
    ack_sys: AckSystem<SD>,
    ordering_sys: OrderingSystem<RD>,
}

impl<SD: Clone, RD, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> ReliableSystem<SD, RD, C, A, R, D> {
    /// Creates a new [`ReliableSystem`].
    pub fn new(msg_table: MsgTable<C, A, R, D>, config: NetConfig) -> Self {
        let m_table_count = msg_table.mtype_count();
        ReliableSystem {
            msg_table,
            last_ack_msg: Instant::now(),
            ack_sys: AckSystem::new(config),
            ordering_sys: OrderingSystem::new(m_table_count),
        }
    }

    /// Handles the reliability aspects of receiving a message other than ordering.
    ///
    /// For ordering, use [`push_received`](Self::push_received) to push the message into the
    /// ordering buffer.
    pub fn msg_received(&mut self, header: MsgHeader) {
        self.ack_sys.msg_received(header.sender_ack_num);
        self.ack_sys
            .mark_bitfield(header.receiver_acking_offset, header.ack_bits);
    }

    /// Handles an incoming [`AckMsg`].
    pub fn recv_ack_msg(&mut self, ack_msg: AckMsg) {
        self.ack_sys.handle_ack_msg(ack_msg)
    }

    /// Pushes the received message into the ordering buffer to be ordered according to
    /// it's guarantees.
    pub fn push_received(&mut self, header: MsgHeader, receive_data: RD) {
        let guarantees = self.msg_table.guarantees[header.m_type];
        self.ordering_sys.push(header, guarantees, receive_data);
    }

    /// Gets an [`AckMsg`] for acknowledging all received messages in the window if one needs to be
    /// sent.
    pub fn get_ack_msg(&mut self) -> Option<AckMsg> {
        if let Some(limit) = ACK_MSG_LIMIT {
            if self.last_ack_msg.elapsed() < limit {
                return None;
            }
            self.last_ack_msg = Instant::now();
        }
        Some(self.ack_sys.ack_msg_info())
    }

    pub fn get_received(&mut self) -> Option<(MsgHeader, RD)> {
        self.ordering_sys.next()
    }

    pub fn get_send_header(&mut self, m_type: MType) -> MsgHeader {
        let (offset, bitfield) = self.ack_sys.next_header();
        MsgHeader {
            m_type,
            order_num: self.ordering_sys.next_outgoing(m_type),
            sender_ack_num: self.ack_sys.outgoing_ack_num(),
            receiver_acking_offset: offset,
            ack_bits: bitfield,
        }
    }

    /// Saves a message, if it is reliable, so that it can be resent if it is lost in transit.
    ///
    /// This also
    pub fn save(&mut self, header: MsgHeader, guarantees: Guarantees, send_data: SD) {
        self.ack_sys.save_msg(header, guarantees, send_data);
    }

    /// Gets messages that are due for a resend.
    pub fn get_resend(&mut self, rtt: u32) -> Vec<(MsgHeader, SD)> {
        self.ack_sys.get_resend(rtt)
    }

    /// Checks to see if the given [`AckNum`] is in the resend buffer.
    /// Therefore, this will return `false` if a message has not been acknowledged,
    /// or was never sent.
    pub fn is_not_acked(&self, ack_num: AckNum) -> bool {
        self.ack_sys.is_not_acked(ack_num)
    }
}
