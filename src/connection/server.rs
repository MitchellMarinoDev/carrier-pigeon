use crate::connection::{ConnectionList, ConnectionListError, NonAckedMsgs, SavedMsg};
use crate::net::{MNum, MsgHeader};
use crate::transport::ServerTransport;
use crate::{CId, MId, MsgTable};
use hashbrown::HashMap;
use log::trace;
use std::any::{type_name, Any, TypeId};
use std::collections::VecDeque;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

/// A wrapper around the the [`ServerTransport`] that adds the reliability and ordering.
pub struct ServerConnection<T: ServerTransport> {
    msg_table: MsgTable,
    transport: T,

    connection_list: ConnectionList,
    pending_connections: VecDeque<(CId, SocketAddr, Box<dyn Any + Send + Sync>)>,

    msg_counter: HashMap<CId, Vec<MNum>>,
    non_acked: HashMap<CId, Vec<NonAckedMsgs>>,

    missing_msg: HashMap<CId, Vec<Vec<u32>>>,
}

impl<T: ServerTransport> ServerConnection<T> {
    pub fn new(msg_table: MsgTable, listen_addr: impl ToSocketAddrs) -> io::Result<Self> {
        let connection_list = ConnectionList::new();
        let transport = T::new(
            listen_addr,
            msg_table.clone(),
            connection_list.addrs.clone(),
        )?;
        trace!(
            "{} listening on {}",
            type_name::<T>(),
            transport
                .listen_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
        );
        Ok(Self {
            msg_table,
            transport,
            pending_connections: VecDeque::new(),
            connection_list,
            msg_counter: HashMap::new(),
            non_acked: HashMap::new(),
            missing_msg: HashMap::new(),
        })
    }

    pub fn send_to<M: Any + Send + Sync>(&mut self, cid: CId, msg: &M) -> io::Result<()> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let addr = self
            .connection_list
            .addr_of(cid)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Invalid CId ({})", cid)))?;

        let tid = TypeId::of::<M>();

        // create the message header
        let mid = self.msg_table.tid_map[&tid];
        let ack_num = self.msg_counter[&cid][mid];
        *self
            .msg_counter
            .get_mut(&cid)
            .expect("cid should be valid")
            .get_mut(mid)
            .expect("mid should be valid") += 1;
        let msg_header = MsgHeader::new(mid, ack_num);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[mid];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[mid];
        if guarantees.reliable() {
            self.send_reliable(cid, addr, mid, ack_num, payload)
        } else {
            self.send_unreliable(addr, mid, payload)
        }
    }

    fn send_reliable(
        &mut self,
        cid: CId,
        addr: SocketAddr,
        mid: MId,
        ack_num: u32,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        self.transport.send_to(addr, mid, payload.clone())?;
        // add the payload to the list of non-acked messages
        self.non_acked
            .get_mut(&cid)
            .expect("cid should be valid")
            .get_mut(mid)
            .expect("mid should be valid")
            .insert(ack_num, SavedMsg::new(payload));
        Ok(())
    }

    fn send_unreliable(&self, addr: SocketAddr, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send_to(addr, mid, payload)
    }

    pub fn recv_from(&mut self) -> io::Result<(CId, MsgHeader, Box<dyn Any + Send + Sync>)> {
        // TODO: handle any reliability stuff here.
        loop {
            let (addr, header, msg) = self.transport.recv_from()?;
            let cid = match self.connection_list.cid_of(addr) {
                None => continue,
                Some(cid) => cid,
            };
            return Ok((cid, header, msg));
        }
    }

    pub fn get_pending_connection(
        &mut self,
    ) -> Option<(CId, SocketAddr, Box<dyn Any + Send + Sync>)> {
        self.pending_connections.pop_front()
    }

    pub fn new_connection(&mut self, addr: SocketAddr) -> Result<(), ConnectionListError> {
        self.connection_list.new_connection(addr)
    }

    pub fn remove_connection(&mut self, cid: CId) -> Result<(), ConnectionListError> {
        self.connection_list.remove_connection(cid)
    }

    pub fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.transport.listen_addr()
    }

    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.connection_list.cids()
    }

    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.connection_list.cid_of(addr)
    }

    pub fn cid_connected(&self, cid: CId) -> bool {
        self.connection_list.cid_connected(cid)
    }

    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.connection_list.addr_of(cid)
    }

    pub fn connection_count(&self) -> usize {
        self.connection_list.connection_count()
    }
}
