use crate::net::{AckNum, MsgHeader};
use crate::Guarantees;
use hashbrown::HashMap;
use log::warn;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::time::{Duration, Instant};

// TODO: add to config
/// The number of times we need to ack something, to consider it acknowledged enough.
const SEND_ACK_THRESHOLD: u32 = 2;

/// The width of the bitfield that is used for acknowledgement.
const BITFIELD_WIDTH: u16 = 32;

/// Saves the bitfield next to a counter for how many times this was acked.
#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Debug)]
pub(crate) struct AckBitfields {
    bitfield: u32,
    send_count: u32,
}

/// The Acknowledgement System.
///
/// This handles generating the acknowledgment part of the header, getting the info needed for the
/// acknowledgment message, and keeping track of an outgoing ack_number.
///
/// Generic parameter `SD` is for "Send Data". It should be the data that you send to the transport
/// other than the header. Since this differs between client and server (server needs to keep track
/// of a to address), it is made a generic parameter.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct AckSystem<SD: Clone> {
    /// The current [`AckNum`] for outgoing messages.
    outgoing_counter: AckNum,
    /// The current ack_offset value for the front end of the buffer.
    /// This is also the lower limit of the ack bitfield window.
    ack_offset: AckNum,
    /// The current index of the ack_bitfields that we are on.
    ///
    /// Used for `get_next`.
    current_idx: usize,
    /// The ack bitfields.
    ///
    /// This stores a bitfield for weather the 32 messages before `ack_offset` have been received.
    ack_bitfields: VecDeque<AckBitfields>,
    /// This stores the saved reliable messages.
    saved_msgs: HashMap<AckNum, (Instant, MsgHeader, SD)>,
}

impl<SD: Clone> AckSystem<SD> {
    /// Creates a new [`AckSystem`].
    pub fn new() -> Self {
        let mut deque = VecDeque::new();
        deque.push_front(AckBitfields::default());
        AckSystem {
            outgoing_counter: 0,
            ack_offset: 0,
            current_idx: usize::MAX - 1,
            ack_bitfields: deque,
            saved_msgs: HashMap::new(),
        }
    }

    /// Marks the [`AckNum`] of a message as received.
    ///
    /// Marks an incoming message as received, so it gets acknowledged in the next message we send.
    pub fn msg_received(&mut self, num: AckNum) {
        // The highest AckNum we can have without shifting.
        let mut upper_bound =
            self.ack_offset + (BITFIELD_WIDTH * self.ack_bitfields.len() as AckNum);
        // shift the ack_bitfields (if needed) to make room for ack_offset
        while num >= upper_bound {
            // if the last element has been acknowledged enough, pop the back to make room.
            // otherwise, we just push one on the front, growing the buffer
            if self.ack_bitfields[0].send_count >= SEND_ACK_THRESHOLD {
                self.ack_bitfields.pop_front();
                // ack_offset keeps track of the offset from the front; when we remove the front,
                // we must increment it.
                self.ack_offset += BITFIELD_WIDTH;
            }
            self.ack_bitfields.push_back(AckBitfields::default());
            // recalculate the upper bound
            upper_bound = self.ack_offset + (BITFIELD_WIDTH * self.ack_bitfields.len() as AckNum);
        }
        if num < self.ack_offset {
            // num is outside the window. This is unlikely to happen unless a packet stays on the
            // internet for a long time.
            warn!(
                "Received a message that is outside of the window (AckNum: {})",
                num
            );
            return;
        }
        let dif = num - self.ack_offset;
        let bit_flag = 1 << (dif % BITFIELD_WIDTH);
        let field_idx = dif / BITFIELD_WIDTH;
        self.ack_bitfields[field_idx as usize].bitfield |= bit_flag;
        // when we modify a bitfield, reset that send count back to 0.
        self.ack_bitfields[field_idx as usize].send_count = 0;
    }

    /// Marks an incoming `ack_offset` and `ack_bitfield` pair. These come in the header of messages
    /// from the peer.
    ///
    /// For marking an incoming single ack,
    /// use [`mark_incoming`](Self::mark_incoming)
    pub fn mark_bitfield(&mut self, offset: AckNum, bitfield: u32) {
        for i in 0..BITFIELD_WIDTH {
            if bitfield & (1 << i) != 0 {
                self.saved_msgs.remove(&(offset + i));
            }
        }
    }

    /// Gets the next ack_offset and bitflags associated with it to be sent in the header.
    pub fn next_header(&mut self) -> (AckNum, u32) {
        self.current_idx = (self.current_idx + 1) % self.ack_bitfields.len();
        let field = self.ack_bitfields[self.current_idx];
        self.ack_bitfields[self.current_idx].send_count += 1;
        (
            self.ack_offset + (BITFIELD_WIDTH * self.current_idx as AckNum),
            field.bitfield,
        )
    }

    /// Gets all the information needed for an ack message.
    ///
    /// This increases the send count for all the bitfields.
    /// Returns the offset and a vec of the bitfields.
    pub fn ack_msg_info(&mut self) -> (AckNum, Vec<u32>) {
        let mut out = Vec::with_capacity(self.ack_bitfields.len());
        for bf in self.ack_bitfields.iter_mut() {
            bf.send_count += 1;
            out.push(bf.bitfield);
        }
        (self.ack_offset, out)
    }

    /// Gets the next outgoing [`AckNum`].
    pub fn outgoing_ack_num(&mut self) -> AckNum {
        let ack = self.outgoing_counter;
        self.outgoing_counter = self.outgoing_counter.wrapping_add(1);
        ack
    }

    /// Saves a reliable message so that it can be sent again later if the message gets lost.
    pub fn save_msg(&mut self, header: MsgHeader, guarantees: Guarantees, other_data: SD) {
        if guarantees.unreliable() {
            return;
        }

        // if the guarantee is ReliableNewest, we only need to guarantee the reliability of the
        // newest message; we should remove an old one if it exists
        if guarantees == Guarantees::ReliableNewest {
            // if there is an existing message of the same m_type in the saved buffer, remove it.
            // TODO: this might work better as a sorted vector.
            let existing_ack = self
                .saved_msgs
                .iter()
                .filter_map(|(ack, (_, saved_header, _))| {
                    if saved_header.m_type == header.m_type {
                        Some(*ack)
                    } else {
                        None
                    }
                })
                .next();
            if let Some(ack) = existing_ack {
                self.saved_msgs.remove(&ack);
            }
        }

        // finally, insert the msg
        self.saved_msgs
            .insert(header.sender_ack_num, (Instant::now(), header, other_data));
    }

    /// Gets messages that are due for a resend. This resets the time sent.
    pub fn get_resend(&mut self) -> Vec<(MsgHeader, SD)> {
        let mut acks = vec![];
        for (sent, msg_header, sd) in self.saved_msgs.values_mut() {
            // TODO: add duration to config.
            if sent.elapsed() > Duration::from_millis(100) {
                *sent = Instant::now();
                acks.push((*msg_header, sd.clone()));
            }
        }

        acks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Guarantees::{Reliable, ReliableNewest};

    #[test]
    fn test_mark_received() {
        let mut ack_system: AckSystem<()> = AckSystem::new();

        ack_system.msg_received(0);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 0); // default
        assert_eq!(ack_system.ack_bitfields.front().unwrap().bitfield, 1 << 0,);

        ack_system.msg_received(8);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 0); // default
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().bitfield,
            1 << 8 | 1 << 0
        );
        assert_eq!(ack_system.next_header(), (0, 1 << 8 | 1 << 0));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);
        // The rest of the test relies on this not being across the threshold
        assert!(ack_system.ack_bitfields[0].send_count < SEND_ACK_THRESHOLD);

        ack_system.msg_received(32 + 6);
        assert_eq!(ack_system.ack_bitfields.len(), 2);
        assert_eq!(ack_system.ack_offset, 0);
        assert_eq!(ack_system.ack_bitfields[1].bitfield, 1 << 6);
        assert_eq!(ack_system.ack_bitfields[1].send_count, 0);
        // Since the first bitfield has already been sent, the next one to send should be the
        // second bitfield.
        assert_eq!(ack_system.next_header(), (32, 1 << 6));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);
    }

    #[test]
    fn test_save_ack() {
        let mut ack_system = AckSystem::new();

        ack_system.save_msg(MsgHeader::new(1, 0, 10, 0, 0), Reliable, ());
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.save_msg(MsgHeader::new(1, 0, 11, 0, 0), Reliable, ());
        assert_eq!(ack_system.saved_msgs.len(), 2);
        ack_system.mark_bitfield(0, 1 << 10);
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.mark_bitfield(0, 1 << 11);
        assert_eq!(ack_system.saved_msgs.len(), 0);

        // check out of order ack
        ack_system.save_msg(MsgHeader::new(1, 0, 20, 0, 0), Reliable, ());
        ack_system.save_msg(MsgHeader::new(1, 0, 21, 0, 0), Reliable, ());
        ack_system.save_msg(MsgHeader::new(1, 0, 22, 0, 0), Reliable, ());
        assert_eq!(ack_system.saved_msgs.len(), 3);
        ack_system.mark_bitfield(0, 1 << 22);
        assert_eq!(ack_system.saved_msgs.len(), 2);
        ack_system.mark_bitfield(0, 1 << 21);
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.mark_bitfield(0, 1 << 20);
        assert_eq!(ack_system.saved_msgs.len(), 0);

        // check mark_bitfield
        ack_system.save_msg(MsgHeader::new(1, 0, 32, 0, 0), Reliable, ());
        ack_system.save_msg(MsgHeader::new(1, 0, 33, 0, 0), Reliable, ());
        ack_system.save_msg(MsgHeader::new(1, 0, 34, 0, 0), Reliable, ());
        ack_system.save_msg(MsgHeader::new(1, 0, 63, 0, 0), Reliable, ());
        assert_eq!(ack_system.saved_msgs.len(), 4);
        ack_system.mark_bitfield(32, 1 << 0 | 1 << 1 | 1 << 2 | 1 << 31);
        assert_eq!(ack_system.saved_msgs.len(), 0);
    }

    #[test]
    fn newest() {
        let mut ack_system = AckSystem::new();

        ack_system.save_msg(MsgHeader::new(1, 0, 10, 0, 0), ReliableNewest, ());
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.save_msg(MsgHeader::new(1, 0, 11, 0, 0), ReliableNewest, ());
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.save_msg(MsgHeader::new(1, 0, 12, 0, 0), ReliableNewest, ());
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.mark_bitfield(0, 1 << 12);
        assert_eq!(ack_system.saved_msgs.len(), 0);
    }

    // TODO: impl and test the AckNum rolling over logic
}
