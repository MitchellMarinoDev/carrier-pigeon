use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use crate::MType;
use crate::net::{AckNum, ErasedNetMsg, MsgHeader, OrderNum};

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
    pub fn push(&mut self, msg_data: MsgData) {
        let header = msg_data.1;
        let current = &mut self.current[header.m_type];
        match header.order_num.cmp(current) {
            Ordering::Equal => {
                // this is the next message in the order; add it to the `next` que
                *current += 1;
                self.next.push_back(msg_data);
                // check to see if any messages in the future are now the next message up
                if let Some(msg_data) = self.future[header.m_type].remove(current) {
                    self.push(msg_data);
                }
            }
            Ordering::Greater => {
                // message is in the future; add it to the future buffer
                self.future[header.m_type].insert(header.order_num, msg_data);
            }
            // message is in the past. Likely a duplicate message.
            Ordering::Less => {}
        }
    }

    /// Gets the next, if any, message data.
    pub fn next(&mut self) -> Option<MsgData> {
        self.next.pop_front()
    }
}