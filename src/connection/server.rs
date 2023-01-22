use crate::connection::ack_system::AckSystem;
use crate::connection::{ConnectionList, ConnectionListError, NonAckedMsgs};
use crate::message_table::{CONNECTION_M_TYPE, RESPONSE_M_TYPE};
use crate::net::{MsgHeader, HEADER_SIZE};
use crate::transport::{ServerTransport};
use crate::{CId, MsgTable};
use hashbrown::HashMap;
use log::{debug, error, trace, warn};
use std::any::{type_name, Any, TypeId};
use std::collections::VecDeque;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::connection::reliable::ReliableSystem;

/// A wrapper around the the [`ServerTransport`] that adds
/// (de)serialization, reliability and ordering to the messages.
pub struct ServerConnection<T: ServerTransport> {
    /// The [`MsgTable`] to use for sending and receiving messages.
    msg_table: MsgTable,
    /// The transport to use to send and receive the messages.
    transport: T,
    /// The [`ReliableSystem`]s to add optional reliability to messages for each connection.
    reliable_sys: HashMap<CId, ReliableSystem<(SocketAddr, Arc<Vec<u8>>), (CId, Box<dyn Any + Send + Sync>)>>,
    /// The connection list for managing the connections to this [`ServerConnection`].
    connection_list: ConnectionList,
    /// A buffer for messages that are ready to be received.
    ready: VecDeque<(CId, MsgHeader, Box<dyn Any + Send + Sync>)>,
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
            reliable_sys: HashMap::new(),
            msg_table,
            transport,
            connection_list,
            ready: VecDeque::new(),
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
        let reliable_sys = self.reliable_sys.get_mut(&cid).expect("cid is already checked");
        let header = reliable_sys.get_send_header(m_type);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(header.to_be_bytes());

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        reliable_sys.save(header, guarantees, (addr, payload.clone()));
        self.transport.send_to(addr, m_type, payload)
    }

    /// Receives a message from the transport.
    ///
    /// This will get the next message that is ready to be yielded (if all ordering conditions are
    /// satisfied).
    ///
    /// A Error of type WouldBlock means no more messages can be returned at this time. Other
    /// errors are errors in receiving or validating the data.
    pub fn recv_from(&mut self) -> io::Result<(CId, MsgHeader, Box<dyn Any + Send + Sync>)> {
        loop {
            // if there is a message that is ready, return it
            if let Some(ready) = self.ready.pop_front() {
                return Ok(ready);
            }
            // otherwise, try to get a new one

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
                    self.connection_list.new_pending(from, msg).expect("address already checked to not be connected");
                    continue;
                }
                Some(cid) => cid,
            };

            let msg = self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..])?;

            // handle reliability and ordering
            let reliable_sys = self.reliable_sys.get_mut(&cid).expect("cid already checked");
            reliable_sys.push_received(header, (cid, msg));
            // get all messages from the reliable system and push them on the "ready" que.
            while let Some((header, (cid, msg))) = reliable_sys.get_received() {
                self.ready.push_back((cid, header, msg));
            }
        }
    }

    /// Resends any messages that it needs to for the reliability system to work.
    pub fn resend_reliable(&mut self) {
        for cid in self.connection_list.cids() {
            let reliable_sys = self.reliable_sys.get_mut(&cid).expect("cid should be valid");
            for (header, (addr, payload)) in reliable_sys.get_resend() {
                if let Err(err) = self.transport.send_to(*addr, header.m_type, payload.clone()) {
                    error!("Error resending msg {}: {}", header.sender_ack_num, err);
                }
            }
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
                if let Err(err) = self.send_to(cid, &response) {
                    warn!("failed to send response message to {} (cid: {}): {}", addr, cid, err);
                }
            }
        }
        Ok(count)
    }

    fn new_connection(&mut self, cid: CId, addr: SocketAddr) -> Result<(), ConnectionListError> {
        self.connection_list.new_connection(cid, addr)?;
        self.reliable_sys.insert(cid, ReliableSystem::new(self.msg_table.clone()));
        Ok(())
    }

    pub fn remove_connection(&mut self, cid: CId) -> Result<(), ConnectionListError> {
        self.connection_list.remove_connection(cid)?;
        let old_reliable = self.reliable_sys.remove(&cid);
        debug_assert!(old_reliable.is_some(), "since self.connection_list.remove_connection() didn't fail, there should be a corresponding entry for that cid in self.reliable_sys");
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
