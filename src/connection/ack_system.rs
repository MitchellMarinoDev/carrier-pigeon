use crate::messages::AckMsg;
use crate::net::{AckNum, MsgHeader};
use crate::{Guarantees, NetConfig};
use hashbrown::HashMap;
use std::cmp::max;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::mem;
use std::time::Instant;

// TODO: impl ack_send_count for residuals

/// The width of the bitfield that is used for acknowledgement.
const BITFIELD_WIDTH: u16 = 32;

/// The bitfield containing the acknowledgment numbers for 32 consecutive AckNums.
///
/// This also stores the `send_count` which is how many times these AckNums were sent.
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
    /// The current offset for our window.
    ///
    /// This offset is the end of the window; the entire window is before this ack offset.
    ack_offset: AckNum,
    /// The current index of the ack_bitfields that we are on.
    ///
    /// Since we only send part of our window every time, we need to rotate through our window.
    /// This keeps track of what part of the window we are on.
    ///
    /// Used for `get_next`.
    current_idx: usize,
    /// The [`NetConfig`].
    config: NetConfig,
    /// A buffer for received messages that dont fit in the bitfield window.
    residuals: Vec<AckNum>,
    /// The ack bitfields.
    ///
    /// This stores a bitfield for weather the 32 messages before `ack_offset` have been received.
    ack_bitfields: VecDeque<AckBitfields>,
    /// This stores the saved reliable messages.
    saved_msgs: HashMap<AckNum, (Instant, MsgHeader, SD)>,
}

impl<SD: Clone> AckSystem<SD> {
    /// Creates a new [`AckSystem`].
    pub fn new(config: NetConfig) -> Self {
        let mut deque = VecDeque::new();
        deque.push_front(AckBitfields::default());
        AckSystem {
            outgoing_counter: 0,
            ack_offset: 0,
            current_idx: usize::MAX - 1,
            config,
            residuals: vec![],
            ack_bitfields: deque,
            saved_msgs: HashMap::new(),
        }
    }

    /// Marks the [`AckNum`] of a message as received.
    ///
    /// Marks an incoming message as received, so it gets acknowledged in the next message we send.
    pub fn msg_received(&mut self, num: AckNum) {
        let window_width = (BITFIELD_WIDTH * self.ack_bitfields.len() as AckNum);
        // The highest AckNum we can have without shifting.
        let mut upper_bound = self.ack_offset.wrapping_add(window_width);

        let distance_away_from_window = max(num.saturating_sub(self.ack_offset), upper_bound.saturating_sub(num));

        // TODO: wrapping logic

        // shift the ack_bitfields (if needed) to make room for the new number
        while num >= upper_bound {
            // if the first element (oldest ack number) has been acknowledged enough,
            // pop it to make room. otherwise, we just push one on the back, growing the buffer
            if self.ack_bitfields[0].send_count >= self.config.ack_send_count {
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
            // TODO: Remove this whole notion of residuals, when we start assigning new ack numbers
            //       to resent messages. In addition, this screening of weather a message can be dumped
            //       will need to happen sooner, because it will need to occur before decryption
            //       as part of the protection against packet replay.
            //       Therefore this "within window" checking logic should be moved to its own function.
            // num is outside the window. This is unlikely to happen unless a packet stays on the
            // internet for a long time.
            self.residuals.push(num);
            return;
        }

        // TODO: This will fail when wrapping
        let dif = num - self.ack_offset;
        let bit_flag = 1 << (dif % BITFIELD_WIDTH);
        let field_idx = dif / BITFIELD_WIDTH;
        self.ack_bitfields[field_idx as usize].bitfield |= bit_flag;
        // when we modify a bitfield, reset that send count back to 0.
        self.ack_bitfields[field_idx as usize].send_count = 0;
    }

    /// Marks an incoming `ack_offset` and `ack_bitfield` pair. These come in the header of messages
    /// from the peer.
    pub fn mark_bitfield(&mut self, offset: AckNum, bitfield: u32) {
        for i in 0..BITFIELD_WIDTH {
            if bitfield & (1 << i) != 0 {
                self.saved_msgs.remove(&(offset + i));
            }
        }
    }

    /// Handles an incoming [`AckMsg`].
    pub fn handle_ack_msg(&mut self, ack_msg: AckMsg) {
        for (i, bitfield) in ack_msg.bitfields.into_iter().enumerate() {
            self.mark_bitfield(ack_msg.ack_offset + BITFIELD_WIDTH * i as AckNum, bitfield);
        }
        for residual in ack_msg.residuals {
            self.saved_msgs.remove(&residual);
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
    pub fn ack_msg_info(&mut self) -> AckMsg {
        let residuals = if self.residuals.is_empty() {
            Vec::with_capacity(0)
        } else {
            mem::take(&mut self.residuals)
        };

        let mut out = Vec::with_capacity(self.ack_bitfields.len());
        for bf in self.ack_bitfields.iter_mut() {
            bf.send_count += 1;
            out.push(bf.bitfield);
        }
        AckMsg::new(self.ack_offset, out, residuals)
    }

    /// Gets the next outgoing [`AckNum`].
    pub fn next_ack_num(&mut self) -> AckNum {
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
            .insert(header.message_ack_num, (Instant::now(), header, other_data));
    }

    /// Gets messages that are due for a resend. This resets the time sent.
    ///
    /// `rtt` should be the round trip time in microseconds.
    pub fn get_resend(&mut self, rtt: u32) -> Vec<(MsgHeader, SD)> {
        let rtt = max(rtt, 800);
        let mut resend = vec![];
        for (sent, msg_header, sd) in self.saved_msgs.values_mut() {
            // TODO: add duration to config.
            if sent.elapsed().as_micros() > (rtt * 3 / 2) as u128 {
                *sent = Instant::now();
                resend.push((*msg_header, sd.clone()));
            }
        }

        resend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Guarantees::{Reliable, ReliableNewest};

    #[test]
    fn test_msg_received() {
        let mut ack_system: AckSystem<()> = AckSystem::new(NetConfig::default());

        ack_system.msg_received(0);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 0); // default
        assert_eq!(ack_system.ack_bitfields.front().unwrap().bitfield, 1 << 0);

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
        assert!(ack_system.ack_bitfields[0].send_count < NetConfig::default().ack_send_count);

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
    fn test_save_msg() {
        let mut ack_system = AckSystem::new(NetConfig::default());

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
    fn test_reliable_newest() {
        let mut ack_system = AckSystem::new(NetConfig::default());

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

    #[test]
    fn test_ack_num_rollover() {
        let mut ack_system: AckSystem<()> = AckSystem::new(NetConfig::default());

        ack_system.msg_received(0u16.wrapping_sub(32));
        panic!("{:?}", ack_system);
    }
}
