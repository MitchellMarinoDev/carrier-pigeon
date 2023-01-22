use crate::net::{MsgHeader, OrderNum};
use crate::MType;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

type MsgData = (SocketAddr, MsgHeader, Vec<u8>);

pub struct OrderingSystem {
    current: Vec<OrderNum>,
    future: Vec<HashMap<OrderNum, MsgData>>,
    next: VecDeque<MsgData>,
}

impl OrderingSystem {
    /// Creates a new [`OrderingSystem`].
    pub fn new(m_type_count: MType) -> Self {
        OrderingSystem {
            current: vec![0; m_type_count],
            future: (0..m_type_count).map(|_| HashMap::new()).collect(),
            next: VecDeque::new(),
        }
    }

    /// Pushes message data into the ordering system.
    ///
    /// The MsgHeader in `msg_data` must already be validated.
    pub fn push(&mut self, from: SocketAddr, header: MsgHeader, payload: Vec<u8>) {
        let current = &mut self.current[header.m_type];
        match header.order_num.cmp(current) {
            Ordering::Equal => {
                // this is the next message in the order; add it to the `next` que
                *current += 1;
                self.next.push_back((from, header, payload));
                // check to see if any messages in the future are now the next message up
                if let Some((from, header, payload)) = self.future[header.m_type].remove(current) {
                    self.push(from, header, payload);
                }
            }
            Ordering::Greater => {
                // message is in the future; add it to the future buffer
                self.future[header.m_type].insert(header.order_num, (from, header, payload));
            }
            // message is in the past. Likely a duplicate message.
            Ordering::Less => {}
        }
    }

    /// For the "Newest" guarantees, check if this message is the newest one received.
    pub fn check_newest(&mut self, header: &MsgHeader) -> bool {
        let current = &mut self.current[header.m_type];
        if header.order_num >= *current {
            *current = header.order_num + 1;
            true
        } else {
            false
        }
    }

    /// Gets the next, if any, message data.
    pub fn next(&mut self) -> Option<MsgData> {
        self.next.pop_front()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ordering() {
        let mut ordering_sys = OrderingSystem::new(12);
        let from = "[::1]:1".parse().unwrap();

        ordering_sys.push(from, MsgHeader::new(1, 0, 0, 0, 0), vec![]);
        assert!(ordering_sys.next().is_some());
        ordering_sys.push(from, MsgHeader::new(1, 2, 1, 0, 0), vec![]);
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(from, MsgHeader::new(1, 1, 2, 0, 0), vec![]);
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
    }

    #[test]
    fn test_across_mtypes() {
        let mut ordering_sys = OrderingSystem::new(12);
        let from = "[::1]:1".parse().unwrap();

        ordering_sys.push(from, MsgHeader::new(1, 0, 0, 0, 0), vec![]);
        ordering_sys.push(from, MsgHeader::new(1, 2, 1, 0, 0), vec![]);
        ordering_sys.push(from, MsgHeader::new(2, 1, 2, 0, 0), vec![]);
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(from, MsgHeader::new(2, 0, 2, 0, 0), vec![]);
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
        ordering_sys.push(from, MsgHeader::new(1, 1, 2, 0, 0), vec![]);
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_some());
        assert!(ordering_sys.next().is_none());
    }

    #[test]
    fn test_newest() {
        let mut ordering_sys = OrderingSystem::new(12);

        assert!(ordering_sys.check_newest(&MsgHeader::new(1, 0, 0, 0, 0)));
        assert!(ordering_sys.check_newest(&MsgHeader::new(1, 1, 0, 0, 0)));
        assert!(ordering_sys.check_newest(&MsgHeader::new(1, 10, 0, 0, 0)));

        assert!(!ordering_sys.check_newest(&MsgHeader::new(1, 9, 0, 0, 0)));
        assert!(!ordering_sys.check_newest(&MsgHeader::new(1, 10, 0, 0, 0)));

        assert!(ordering_sys.check_newest(&MsgHeader::new(2, 0, 0, 0, 0)));
    }
}
