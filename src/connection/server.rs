use crate::connection::{ConnectionList, NonAckedMsgs, SavedMsg};
use crate::net::{MNum, MsgHeader};
use crate::transport::ServerTransport;
use crate::{CId, MId, MsgTable};
use hashbrown::HashMap;
use log::trace;
use std::any::{type_name, Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};

/// A wrapper around the the [`ServerTransport`] that adds the reliability and ordering.
pub struct ServerConnection<T: ServerTransport> {
    msg_table: MsgTable,
    transport: T,

    connection_list: ConnectionList,

    msg_counter: HashMap<CId, Vec<MNum>>,
    non_acked: Mutex<HashMap<CId, Vec<NonAckedMsgs>>>,

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
            connection_list,
            msg_counter: HashMap::new(),
            non_acked: Mutex::new(HashMap::new()),
            missing_msg: HashMap::new(),
        })
    }

    pub fn send_to<M: Any + Send + Sync>(&self, cid: CId, msg: &M) -> io::Result<()> {
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
        self.msg_counter[&cid][mid] += 1;
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
        &self,
        cid: CId,
        addr: SocketAddr,
        mid: MId,
        ack_num: u32,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        self.transport.send_to(addr, mid, payload.clone())?;
        // add the payload to the list of non-acked messages
        {
            let mut non_acked = self.non_acked.lock().expect("failed to obtain lock");
            non_acked[&cid][mid].insert(ack_num, SavedMsg::new(payload));
        }
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
