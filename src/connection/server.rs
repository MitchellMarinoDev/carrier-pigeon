use crate::connection::ack_system::AckSystem;
use crate::connection::{ConnectionList, ConnectionListError, NonAckedMsgs, SavedMsg};
use crate::message_table::{CONNECTION_M_TYPE, RESPONSE_M_TYPE};
use crate::net::{AckNum, MsgHeader, OrderNum, HEADER_SIZE};
use crate::transport::ServerTransport;
use crate::{CId, MType, MsgTable};
use hashbrown::HashMap;
use log::{debug, trace, warn};
use std::any::{type_name, Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

/// A wrapper around the the [`ServerTransport`] that adds the reliability and ordering.
pub struct ServerConnection<T: ServerTransport> {
    msg_table: MsgTable,
    transport: T,

    connection_list: ConnectionList,

    // TODO: combine all these hashmaps.
    // TODO: Then, combine logic in this inner type to reduce code duplication.
    /// A MType-independent, per-connection, counter for acknowledgment numbers.
    ack_num: HashMap<CId, AckNum>,
    /// A per-MType counter used for ordering.
    order_num: HashMap<CId, Vec<OrderNum>>,
    msg_counter: HashMap<CId, Vec<OrderNum>>,
    non_acked: HashMap<CId, Vec<NonAckedMsgs>>,
    remote_ack: HashMap<CId, AckSystem>,

    missing_msg: HashMap<CId, Vec<Vec<u32>>>,
}

impl<T: ServerTransport> ServerConnection<T> {
    pub fn new(msg_table: MsgTable, listen_addr: SocketAddr) -> io::Result<Self> {
        let connection_list = ConnectionList::new();
        let transport = T::new(listen_addr)?;
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
            ack_num: HashMap::new(),
            order_num: HashMap::new(),
            msg_counter: HashMap::new(),
            non_acked: HashMap::new(),
            remote_ack: HashMap::new(),
            missing_msg: HashMap::new(),
        })
    }

    pub fn send_to<M: Any + Send + Sync>(&mut self, cid: CId, msg: &M) -> io::Result<()> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let addr = self
            .connection_list
            .addr_of(cid)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Invalid CId: {}", cid)))?;

        let tid = TypeId::of::<M>();

        // create the message header
        let m_type = self.msg_table.tid_map[&tid];
        let order_num = self
            .order_num
            .get_mut(&cid)
            .expect("cid is already checked")
            .get_mut(m_type)
            .expect("m_type is already checked");
        *order_num += 1;
        let sender_ack_num = self.ack_num[&cid];
        *self.ack_num.get_mut(&cid).expect("cid is already checked") += 1;
        let (receiver_acking_num, ack_bits) = self
            .remote_ack
            .get_mut(&cid)
            .expect("cid is already checked")
            .get_next();
        let msg_header = MsgHeader::new(
            m_type,
            *order_num,
            sender_ack_num,
            receiver_acking_num,
            ack_bits,
        );

        // build the payload using the header and the message
        let mut payload = Vec::from(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        if guarantees.reliable() {
            self.send_reliable(cid, addr, m_type, sender_ack_num, payload)
        } else {
            self.send_unreliable(addr, m_type, payload)
        }
    }

    fn send_reliable(
        &mut self,
        cid: CId,
        addr: SocketAddr,
        m_type: MType,
        sender_ack_number: AckNum,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        self.transport.send_to(addr, m_type, payload.clone())?;
        self.non_acked
            .get_mut(&cid)
            .expect("cid is already checked")
            .get_mut(m_type)
            .expect("m_type already checked")
            .insert(sender_ack_number, SavedMsg::new(payload));
        Ok(())
    }

    fn send_unreliable(
        &self,
        addr: SocketAddr,
        m_type: MType,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        self.transport.send_to(addr, m_type, payload)
    }

    pub fn recv_from(&mut self) -> io::Result<(CId, MsgHeader, Box<dyn Any + Send + Sync>)> {
        let (cid, header, msg) = self.raw_recv_from()?;
        // TODO: handle any reliability stuff here.
        Ok((cid, header, msg))
    }

    /// Receives a message from the transport.
    ///
    /// This function converts the address to a CId and drops any messages from not connected
    /// clients that arent of the type [`CONNECTION_M_TYPE`]. This also validates the MType.
    /// This does not handle reliablility at all, just discards unneeded messages.
    fn raw_recv_from(&mut self) -> io::Result<(CId, MsgHeader, Box<dyn Any + Send + Sync>)> {
        loop {
            let (from, buf) = self.transport.recv_from()?;
            let n = buf.len();
            if n < HEADER_SIZE {
                warn!(
                    "Server: Received a packet of length {} from {} which is not big enough \
                    to be a carrier pigeon message. Discarding",
                    n, from
                );
                continue;
            }
            let header = MsgHeader::from_be_bytes(&buf[..HEADER_SIZE]);
            self.msg_table.check_m_type(header.m_type)?;
            trace!(
                "Server: received message with MType: {}, len: {}, from: {}.",
                header.m_type,
                n,
                from
            );

            let cid = match self.connection_list.cid_of(from) {
                // the message received was not from a connected client
                None => {
                    // ignore messages from not connected clients,
                    // unless it is a connection type message
                    if header.m_type != CONNECTION_M_TYPE {
                        debug!(
                            "Server: Discarding a message that not a connection \
                        message from a non-client ({})",
                            from
                        );
                        continue;
                    }

                    debug!("Server: Connection message from {}", from);
                    let msg = self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..])?;
                    // create a new connection
                    let _ = self.connection_list.new_pending(from, msg);
                    continue;
                }
                Some(cid) => cid,
            };

            let msg = self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..])?;

            // TODO: handle any reliability stuff here

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
        if self.msg_table.tid_map.get(&c_tid) != Some(&CONNECTION_M_TYPE) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "generic type `C` needs to the same `C` \
                    that you passed into `MsgTableBuilder::build`"
                    .to_string(),
            ));
        }
        if self.msg_table.tid_map.get(&r_tid) != Some(&RESPONSE_M_TYPE) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "generic type `R` needs to the same `R` \
                    that you passed into `MsgTableBuilder::build`"
                    .to_string(),
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
        let m_type_count = self.msg_table.mtype_count();

        self.ack_num.insert(cid, 0);
        self.order_num.insert(cid, vec![0; m_type_count]);
        self.missing_msg
            .insert(cid, (0..m_type_count).map(|_| vec![]).collect());
        self.msg_counter.insert(cid, vec![0; m_type_count]);
        self.remote_ack.insert(cid, AckSystem::new());
        self.non_acked.insert(
            cid,
            (0..m_type_count).map(|_| NonAckedMsgs::new()).collect(),
        );
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
