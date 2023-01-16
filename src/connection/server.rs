use crate::connection::{ConnectionList, ConnectionListError, NonAckedMsgs, SavedMsg};
use crate::message_table::{CONNECTION_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::MsgHeader;
use crate::transport::ServerTransport;
use crate::{CId, MId, MsgTable};
use hashbrown::HashMap;
use log::trace;
use std::any::{type_name, Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// A wrapper around the the [`ServerTransport`] that adds the reliability and ordering.
pub struct ServerConnection<T: ServerTransport> {
    msg_table: MsgTable,
    transport: T,

    connection_list: ConnectionList,

    msg_counter: HashMap<CId, Vec<AtomicU32>>,
    non_acked: HashMap<CId, Vec<Mutex<NonAckedMsgs>>>,

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
            non_acked: HashMap::new(),
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
        let ack_num = self.msg_counter[&cid][mid].fetch_add(1, Ordering::AcqRel);
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
        {
            // add the payload to the list of non-acked messages
            let mut non_acked = self.non_acked[&cid][mid]
                .lock()
                .expect("should be able to obtain lock");
            non_acked.insert(ack_num, SavedMsg::new(payload));
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
                // the message received was not from a connected client
                None => {
                    // ignore messages from not connected clients,
                    // unless it is a connection type message
                    if header.mid != CONNECTION_TYPE_MID {
                        continue;
                    }
                    // create a new connection
                    let _ = self.connection_list.new_pending(addr, msg);
                    continue;
                }
                Some(cid) => cid,
            };
            return Ok((cid, header, msg));
        }
    }

    /// Handles all outstanding pending connections
    /// by calling `hook` with the `CId`, `SocketAddr` and the connection message.
    ///
    /// ### Errors
    /// Returns an error iff generic parameters `C` and `R` are not the same `C` and `R`
    /// that you passed into [`MsgTableBuilder::build`](crate::MsgTableBuilder::build).
    pub fn handle_pending<C: Any + Send + Sync, R: Any + Send + Sync>(
        &mut self,
        mut hook: impl FnMut(CId, SocketAddr, C) -> (bool, R),
    ) -> io::Result<u32> {
        let c_tid = TypeId::of::<C>();
        let r_tid = TypeId::of::<R>();
        if self.msg_table.tid_map.get(&c_tid) != Some(&CONNECTION_TYPE_MID) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "generic type `C` needs to the same `C` \
                    that you passed into `MsgTableBuilder::build`"
                ),
            ));
        }
        if self.msg_table.tid_map.get(&r_tid) != Some(&RESPONSE_TYPE_MID) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "generic type `R` needs to the same `R` \
                    that you passed into `MsgTableBuilder::build`"
                ),
            ));
        }

        let mut count = 0;
        while let Some((cid, addr, msg)) = self.connection_list.get_pending() {
            if self.connection_list.addr_connected(addr) {
                // address is already connected; ignore the connection request
                continue;
            }

            let msg = *msg.downcast().expect("type `C` should be the correct type");
            count += 1;
            let (accept, response) = hook(cid, addr, msg);
            if accept {
                self.new_connection(cid, addr).expect(
                    "cid and address should be valid, as they came from the connection list",
                );
                // TODO: in the future, send_to should not return an error.
                let _ = self.send_to(cid, &response);
            }
        }
        Ok(count)
    }

    fn new_connection(&mut self, cid: CId, addr: SocketAddr) -> Result<(), ConnectionListError> {
        self.connection_list.new_connection(cid, addr)?;
        let mid_count = self.msg_table.mid_count();

        self.missing_msg.insert(cid, (0..mid_count).map(|_| vec![]).collect());
        self.msg_counter.insert(cid, (0..mid_count).map(|_| AtomicU32::new(0)).collect());
        self.non_acked.insert(cid, (0..mid_count).map(|_| Mutex::new(NonAckedMsgs::new())).collect());
        Ok(())
    }

    pub fn remove_connection(&mut self, cid: CId) -> Result<(), ConnectionListError> {
        self.connection_list.remove_connection(cid)?;

        self.missing_msg.remove(&cid);
        self.msg_counter.remove(&cid);
        self.non_acked.remove(&cid);
        Ok(())
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
