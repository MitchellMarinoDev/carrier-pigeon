use crate::net::{AckNum, MsgHeader};
use std::collections::VecDeque;
use std::time::Instant;
use hashbrown::HashMap;

// TODO: add to config
/// The number of times we need to ack something, to consider it acknowledged enough.
const SEND_ACK_THRESHOLD: u32 = 2;

/// The width of the bitfield that is used for acknowledgement.
const BITFIELD_WIDTH: u32 = 32;

#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Debug)]
pub(crate) struct AckBitfields {
    flags: u32,
    send_count: u32,
}

/// The Acknowledgement System.
///
/// Generic parameter `O` is for "OtherData", which is the data that should be stored along side
/// the header. This is so, if this is used by a client, this can store the payload.
/// If this is used by a server, this can store the payload and from address.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct AckSystem<O> {
    /// The current [`AckNum`] for outgoing messages.
    outgoing_counter: AckNum,
    /// The current ack_offset value.
    ack_offset: AckNum,
    /// The current index of the ack_bitfields that we are on.
    ///
    /// Used for `get_next`.
    current_idx: usize,
    /// The ack bitfields.
    ///
    /// This stores a bitfield for weather the 32 messages before `ack_offset` have been received.
    ack_bitfields: VecDeque<AckBitfields>,
    /// This stores additional acks that are too old to fit in the bitfield. [`AckNums`] might get
    /// put in this buffer if they get lost and must be resent one or more times.
    residual: Vec<AckNum>,
    /// This stores the saved reliable messages.
    saved_msgs: HashMap<AckNum, (Instant, MsgHeader, O)>
}

impl<O> AckSystem<O> {
    /// Creates a new [`AckSystem`].
    pub fn new() -> Self {
        let mut deque = VecDeque::new();
        deque.push_front(AckBitfields::default());
        AckSystem {
            outgoing_counter: 0,
            ack_offset: 0,
            current_idx: 0,
            ack_bitfields: deque,
            residual: vec![],
            saved_msgs: HashMap::new(),
        }
    }

    /// Marks a [`AckNum`] as received.
    ///
    /// Marks an incoming message as received, so it gets acknowledged in the next message we send.
    pub fn mark_received(&mut self, num: AckNum) {
        // shift the ack_bitfields (if needed) to make room for ack_offset
        while num >= self.ack_offset + 32 {
            // if the last element has been acknowledged enough, pop the back to make room.
            // otherwise, we just push one on the front, growing the buffer
            if self.ack_bitfields[self.ack_bitfields.len() - 1].send_count >= SEND_ACK_THRESHOLD {
                self.ack_bitfields.pop_back();
            }
            self.ack_bitfields.push_front(AckBitfields::default());
            self.ack_offset += 32;
        }
        // The lowest number that fits in the bitfield
        let lower_bound = self.ack_offset - (32 * (self.ack_bitfields.len() as AckNum - 1));
        if num < lower_bound {
            // num is outside the window. Add it to the residual to catch it.
            self.residual.push(num);
            return;
        }
        let dif = num - self.ack_offset;
        let field_idx = dif / 32;
        let bit_flag = 1 << (dif % 32);
        self.ack_bitfields[field_idx as usize].flags |= bit_flag;
    }

    /// Marks one of the outgoing messages as acknowledged. That is, an ack from the peer,
    /// for a message that was sent from the this computer.
    ///
    /// For marking a `ack_offset` and `ack_bitfield` pair,
    /// use [`mark_bitfield`](Self::mark_bitfield)
    pub fn mark_outgoing(&mut self, num: AckNum) {
        self.saved_msgs.remove(&num);
    }

    /// Marks an incoming `ack_offset` and `ack_bitfield` pair. These come in the header of messages
    /// from the peer.
    ///
    /// For marking an incoming single ack,
    /// use [`mark_incoming`](Self::mark_incoming)
    pub fn mark_bitfield(&mut self, offset: AckNum, bitfield: u32) {
        for i in 0..32 {
            if bitfield & (1 << i) != 0 {
                self.saved_msgs.remove(&(offset + i));
            }
        }
    }

    /// Gets the next ack_offset and bitflags associated with it to be sent in the header.
    pub fn next_header(&mut self) -> (AckNum, u32) {
        let field = self.ack_bitfields[self.current_idx];
        self.ack_bitfields[self.current_idx].send_count += 1;
        self.current_idx = (self.current_idx + 1) % self.ack_bitfields.len();
        (self.ack_offset, field.flags)
    }

    /// Gets all the information needed for an ack message.
    ///
    /// This increases the send count for all the bitfields, and gets a reference
    /// to the bitfields and a slice to the residual ack numbers.
    pub fn ack_msg_info(&mut self) -> (&VecDeque<AckBitfields>, &[AckNum]) {
        for bf in self.ack_bitfields.iter_mut() {
            bf.send_count += 1;
        }
        (&self.ack_bitfields, &self.residual[..])
    }

    /// Gets the next outgoing [`AckNum`].
    pub fn outgoing_ack_num(&mut self) -> AckNum {
        let ack = self.outgoing_counter;
        self.outgoing_counter = self.outgoing_counter.wrapping_add(1);
        ack
    }

    /// Saves a reliable message so that it can be sent again later if the message gets lost.
    pub fn save_msg(&mut self, header: MsgHeader, other_data: O) {
        self.saved_msgs.insert(header.sender_ack_num, (Instant::now(), header, other_data));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_received() {
        let mut ack_system: AckSystem<()> = AckSystem::new();

        ack_system.mark_received(0);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 0); // default
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().flags,
            1 << 0,
        );

        ack_system.mark_received(8);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 0); // default
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().flags,
            1 << 8 | 1 << 0
        );
        assert_eq!(ack_system.next_header(), (0, 1 << 8 | 1 << 0));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);

        ack_system.mark_received(32 + 6);
        assert_eq!(ack_system.ack_bitfields.len(), 2);
        assert_eq!(ack_system.ack_offset, 32);
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().flags,
            1 << 6
        );
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.next_header(), (32, 1 << 6));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);
    }

    #[test]
    fn test_save_ack() {
        let mut ack_system = AckSystem::new();

        ack_system.save_msg(MsgHeader::new(1, 0, 10, 0, 0), ());
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.save_msg(MsgHeader::new(1, 0, 11, 0, 0), ());
        assert_eq!(ack_system.saved_msgs.len(), 2);
        ack_system.mark_outgoing(10);
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.mark_outgoing(11);
        assert_eq!(ack_system.saved_msgs.len(), 0);

        // check out of order ack
        ack_system.save_msg(MsgHeader::new(1, 0, 20, 0, 0), ());
        ack_system.save_msg(MsgHeader::new(1, 0, 21, 0, 0), ());
        ack_system.save_msg(MsgHeader::new(1, 0, 22, 0, 0), ());
        assert_eq!(ack_system.saved_msgs.len(), 3);
        ack_system.mark_outgoing(22);
        assert_eq!(ack_system.saved_msgs.len(), 2);
        ack_system.mark_outgoing(21);
        assert_eq!(ack_system.saved_msgs.len(), 1);
        ack_system.mark_outgoing(20);
        assert_eq!(ack_system.saved_msgs.len(), 0);

        // check mark_bitfield
        fn bitfield_value(v: AckNum) -> u32 {
            let v = v as u32 % 32;
            1 << v
        }

        ack_system.save_msg(MsgHeader::new(1, 0, 32, 0, 0), ());
        ack_system.save_msg(MsgHeader::new(1, 0, 33, 0, 0), ());
        ack_system.save_msg(MsgHeader::new(1, 0, 34, 0, 0), ());
        ack_system.save_msg(MsgHeader::new(1, 0, 63, 0, 0), ());
        assert_eq!(ack_system.saved_msgs.len(), 4);
        ack_system.mark_bitfield(32, 1 << 0 | 1 << 1 | 1 << 2 | 1 << 31);
        assert_eq!(ack_system.saved_msgs.len(), 0);
    }
}
