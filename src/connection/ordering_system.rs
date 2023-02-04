use crate::net::{MsgHeader, OrderNum};
use crate::{Guarantees, MType};
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

/// The Ordering System.
///
/// This buffers messages and orders them according to their [`Guarantees`].
///
/// Generic parameter `RD` is for "Receive Data". It should be the data that you get from the
/// transport other than the header. Since this differs between client and server
/// (server needs to keep track of a from address), it is made a generic parameter.
pub(crate) struct OrderingSystem<RD> {
    /// Counters for the [`OrderNum`]s of outgoing messages.
    outgoing: Vec<OrderNum>,
    /// Counters for the current [`OrderNum`]s of incoming messages.
    current: Vec<OrderNum>,
    /// Messages that are waiting on older messages. Used for the "ordered" guarantees.
    future: Vec<HashMap<OrderNum, (MsgHeader, RD)>>,
    /// Messages that are not waiting on any other messages.
    next: VecDeque<(MsgHeader, RD)>,
}

impl<RD> OrderingSystem<RD> {
    /// Creates a new [`OrderingSystem`].
    pub fn new(m_type_count: MType) -> Self {
        OrderingSystem {
            outgoing: vec![0; m_type_count],
            current: vec![0; m_type_count],
            future: (0..m_type_count).map(|_| HashMap::new()).collect(),
            next: VecDeque::new(),
        }
    }

    /// Pushes message data into the ordering system.
    ///
    /// The MsgHeader in `msg_data` must already be validated.
    pub fn push(&mut self, header: MsgHeader, guarantees: Guarantees, other_data: RD) {
        if guarantees.ordered() {
            self.handle_ordered(header, other_data);
        } else if guarantees.newest() {
            self.handle_newest(header, other_data);
        } else {
            // TODO: add dupe detection for reliable messages
            self.next.push_back((header, other_data));
        }
    }

    fn handle_ordered(&mut self, header: MsgHeader, other_data: RD) {
        let current = &mut self.current[header.m_type];
        match header.order_num.cmp(current) {
            Ordering::Equal => {
                // this is the next message in the order; add it to the `next` que
                *current += 1;
                self.next.push_back((header, other_data));
                // check to see if the next message in the future is now the next message up
                if let Some((header, other_data)) = self.future[header.m_type].remove(current) {
                    self.handle_ordered(header, other_data);
                }
            }
            Ordering::Greater => {
                // message is in the future; add it to the future buffer
                self.future[header.m_type].insert(header.order_num, (header, other_data));
            }
            // message is in the past. Likely a duplicate message.
            Ordering::Less => {}
        }
    }

    /// For the "Newest" guarantees, check if this message is the newest one received.
    fn handle_newest(&mut self, header: MsgHeader, other_data: RD) {
        let current = &mut self.current[header.m_type];
        if header.order_num < *current {
            return;
        }

        *current = header.order_num + 1;
        // if there is an existing message of the same m_type in the next buffer,
        // replace it with this one.
        for (next_header, next_other_data) in self.next.iter_mut() {
            if next_header.m_type == header.m_type {
                *next_header = header;
                *next_other_data = other_data;
                return;
            }
        }
        // otherwise, push it on the back
        self.next.push_back((header, other_data));
    }

    /// Gets the next, if any, message data.
    pub fn next(&mut self) -> Option<(MsgHeader, RD)> {
        self.next.pop_front()
    }

    pub fn next_outgoing(&mut self, m_type: MType) -> OrderNum {
        let order_num = self.outgoing[m_type];
        self.outgoing[m_type] = self.outgoing[m_type].wrapping_add(1);
        order_num
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ordering() {
        let mut ordering_sys = OrderingSystem::new(12);

        ordering_sys.push(
            MsgHeader::new(1, 0, 0, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_some());
        ordering_sys.push(
            MsgHeader::new(1, 2, 1, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(1, 1, 2, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
    }

    #[test]
    fn test_across_mtypes() {
        let mut ordering_sys = OrderingSystem::new(12);

        ordering_sys.push(
            MsgHeader::new(1, 0, 0, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        ordering_sys.push(
            MsgHeader::new(1, 2, 1, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        ordering_sys.push(
            MsgHeader::new(2, 1, 2, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(2, 0, 2, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(1, 1, 2, 0, 0),
            Guarantees::ReliableOrdered,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
    }

    #[test]
    fn test_newest() {
        let mut ordering_sys = OrderingSystem::new(12);

        ordering_sys.push(
            MsgHeader::new(1, 0, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(1, 1, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(1, 10, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());

        ordering_sys.push(
            MsgHeader::new(1, 9, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(
            MsgHeader::new(1, 10, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_none());

        ordering_sys.push(
            MsgHeader::new(2, 0, 0, 0, 0),
            Guarantees::ReliableNewest,
            (),
        );
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
    }

    // TODO: impl and test the OrderNum rolling over logic
}
