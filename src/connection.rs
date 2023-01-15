use crate::net::MsgHeader;
use crate::transport::{ClientTransport, ServerTransport};
use crate::{CId, MId, MsgTable};
use hashbrown::HashMap;
use std::any::{Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub struct ClientConnection<T: ClientTransport> {
    msg_table: MsgTable,
    transport: T,

    msg_counter: Vec<u32>,
    non_acked: Vec<HashMap<u32, (Arc<Vec<u8>>, Instant)>>,

    missing_msg: Vec<Vec<u32>>,
}

impl<T: ClientTransport> ClientConnection<T> {
    pub fn new(msg_table: MsgTable, transport: T) -> Self {
        let len = msg_table.mid_count();
        let msg_counter = vec![0; len];
        let non_acked = (0..len).map(|_| HashMap::with_capacity(0)).collect();
        let missing_msg = (0..len).map(|_| Vec::with_capacity(0)).collect();
        Self {
            msg_table,
            transport,
            msg_counter,
            non_acked,
            missing_msg,
        }
    }

    pub fn send<M: Any + Send + Sync>(&self, msg: M) -> io::Result<()> {
        // verify type is valid
        let tid = TypeId::of::<M>();
        if !self.msg_table.valid_tid(tid) {
            return Err(Error::new(ErrorKind::InvalidData, "Type not registered."));
        }

        // create the message header
        let mid = self.msg_table.tid_map[&tid];
        let ack_num = self.msg_counter[mid];
        self.msg_counter[mid] += 1;
        let msg_header = MsgHeader::new(mid, ack_num);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[mid];
        ser_fn(&msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[mid];
        if guarantees.reliable() {
            self.send_reliable(mid, ack_num, payload)
        } else {
            self.send_unreliable(mid, payload)
        }
    }

    fn send_reliable(&self, mid: MId, ack_num: u32, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload.clone())?;
        // add the payload to the list of non-acked messages
        self.non_acked[mid].insert(ack_num, (payload, Instant::now()));
        Ok(())
    }

    fn send_unreliable(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload)
    }

    pub fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        // TODO: handle any reliability stuff here.
        self.transport.recv()
    }

    pub fn recv_blocking(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        // TODO: handle any reliability stuff here.
        self.transport.recv_blocking()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.transport.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.transport.peer_addr()
    }
}


/// A wrapper around the the [`ServerTransport`] that adds the reliability and ordering.
pub struct ServerConnection<T: ServerTransport> {
    msg_table: MsgTable,
    transport: T,

    // TODO: maybe HashMap<CId, Vec<u32>> would work better
    msg_counter: Vec<HashMap<CId, u32>>,
    non_acked: Vec<HashMap<u32, (Arc<Vec<u8>>, Instant)>>,

    missing_msg: Vec<Vec<u32>>,
}

impl<T: ServerTransport> ServerConnection<T> {
    pub fn new(msg_table: MsgTable, transport: T) -> Self {
        let len = msg_table.mid_count();
        let msg_counter = vec![0; len];
        let non_acked = (0..len).map(|_| HashMap::with_capacity(0)).collect();
        let missing_msg = (0..len).map(|_| Vec::with_capacity(0)).collect();
        Self {
            msg_table,
            transport,
            msg_counter,
            non_acked,
            missing_msg,
        }
    }

    pub fn send<M: Any + Send + Sync>(&self, msg: M) -> io::Result<()> {
        // verify type is valid
        let tid = TypeId::of::<M>();
        if !self.msg_table.valid_tid(tid) {
            return Err(Error::new(ErrorKind::InvalidData, "Type not registered."));
        }

        // create the message header
        let mid = self.msg_table.tid_map[&tid];
        let ack_num = self.msg_counter[mid];
        self.msg_counter[mid] += 1;
        let msg_header = MsgHeader::new(mid, ack_num);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[mid];
        ser_fn(&msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[mid];
        if guarantees.reliable() {
            self.send_reliable(mid, ack_num, payload)
        } else {
            self.send_unreliable(mid, payload)
        }
    }

    fn send_reliable(&self, mid: MId, ack_num: u32, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload.clone())?;
        // add the payload to the list of non-acked messages
        self.non_acked[mid].insert(ack_num, (payload, Instant::now()));
        Ok(())
    }

    fn send_unreliable(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload)
    }

    pub fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        // TODO: handle any reliability stuff here.
        self.transport.recv()
    }

    pub fn recv_blocking(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        // TODO: handle any reliability stuff here.
        self.transport.recv_blocking()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.transport.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.transport.peer_addr()
    }
}

