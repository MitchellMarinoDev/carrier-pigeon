use crate::net::AckNum;
use std::collections::VecDeque;

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

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default)]
pub(crate) struct AckSystem {
    ack_offset: u16,
    current_idx: usize,
    ack_bitfields: VecDeque<AckBitfields>,
    residual: Vec<u16>,
}

impl AckSystem {
    pub fn new() -> Self {
        let mut deque = VecDeque::new();
        deque.push_front(AckBitfields::default());
        AckSystem {
            ack_offset: 32,
            current_idx: 0,
            ack_bitfields: deque,
            residual: vec![],
        }
    }

    /// Marks a AckNumber as received.
    pub fn mark(&mut self, num: AckNum) {
        // shift the ack_bitfields (if needed) to make room for ack_offset
        while num > self.ack_offset {
            // if the last element has been acknowledged enough, pop the back to make room.
            // otherwise, we just push one on the front, growing the buffer
            if self.ack_bitfields[self.ack_bitfields.len() - 1].send_count >= SEND_ACK_THRESHOLD {
                self.ack_bitfields.pop_back();
            }
            self.ack_bitfields.push_front(AckBitfields::default());
            self.ack_offset += 32;
        }
        let window_width = 32 * self.ack_bitfields.len() as u16;
        if num < self.ack_offset - window_width {
            // num is outside the window. Add it to the residual to catch it.
            self.residual.push(num);
            return;
        }
        let dif = self.ack_offset - num;
        let field_idx = dif / 32;
        let bit_flag = 1 << (dif % 32);
        self.ack_bitfields[field_idx as usize].flags |= bit_flag;
    }

    /// Gets the next ack_offset and bitflags associated with it.
    pub fn get_next(&mut self) -> (u16, u32) {
        let field = self.ack_bitfields[self.current_idx];
        self.ack_bitfields[self.current_idx].send_count += 1;
        self.current_idx = (self.current_idx + 1) % self.ack_bitfields.len();
        (self.ack_offset, field.flags)
    }

    /// Gets all the information needed for an ack message.
    ///
    /// This increases the send count for all the bitfields, and gets a reference
    /// to the bitfields and a slice to the residual ack numbers.
    pub fn get_ack_message(&mut self) -> (&VecDeque<AckBitfields>, &[AckNum]) {
        for bf in self.ack_bitfields.iter_mut() {
            bf.send_count += 1;
        }
        (&self.ack_bitfields, &self.residual[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark() {
        let mut ack_system = AckSystem::new();

        ack_system.mark(8);
        assert_eq!(ack_system.ack_bitfields.len(), 1);
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.ack_offset, 32); // default
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().flags,
            1 << (32 - 8)
        );
        assert_eq!(ack_system.get_next(), (32, 1 << (32 - 8)));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);

        ack_system.mark(32 + 6);
        assert_eq!(ack_system.ack_bitfields.len(), 2);
        assert_eq!(ack_system.ack_offset, 64);
        assert_eq!(
            ack_system.ack_bitfields.front().unwrap().flags,
            1 << (32 - 6)
        );
        assert_eq!(ack_system.ack_bitfields[0].send_count, 0);
        assert_eq!(ack_system.get_next(), (64, 1 << (32 - 6)));
        assert_eq!(ack_system.ack_bitfields[0].send_count, 1);
    }
}
