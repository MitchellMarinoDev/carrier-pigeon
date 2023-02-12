use crate::connection::server_connection::ServerConnection;
use crate::message_table::MsgTable;
use crate::messages::{NetMsg, Response};
use crate::net::{CId, CIdSpec, ErasedNetMsg, Message, ServerConfig, Status};
use crate::transport::server_std_udp::UdpServerTransport;
use log::*;
use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};
use std::io;
use std::io::ErrorKind::WouldBlock;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

/// A server that manages connections to multiple clients.
///
/// Listens on a address and port, allowing for clients to connect. Newly connected clients will
/// be given a connection ID ([`CId`]) starting at `1` that is unique for the session.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Server<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    /// The configuration of the server.
    config: ServerConfig,
    /// The received message buffer.
    ///
    /// Each [`MType`](crate::MType) has its own vector.
    msg_buf: Vec<Vec<ErasedNetMsg>>,

    /// Disconnected connections.
    disconnected: VecDeque<(CId, Status)>,
    /// The connection for this server.
    connection: ServerConnection<UdpServerTransport, C, A, R, D>,

    /// The [`MsgTable`] to use for sending messages.
    msg_table: MsgTable<C, A, R, D>,
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Server<C, A, R, D> {
    /// Creates a new [`Server`].
    pub fn new(
        listen_addr: SocketAddr,
        msg_table: MsgTable<C, A, R, D>,
        config: ServerConfig,
    ) -> io::Result<Self> {
        let connection = ServerConnection::new(msg_table.clone(), listen_addr)?;
        debug!(
            "Creating server listening on {}",
            connection
                .listen_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".into())
        );

        let m_type_count = msg_table.tid_map.len();
        let msg_buf = (0..m_type_count).map(|_| vec![]).collect();

        Ok(Server {
            config,
            msg_buf,
            disconnected: VecDeque::new(),
            connection,
            msg_table,
        })
    }

    /// Gets the config of the server.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Disconnects from the given `cid`. You should always disconnect all clients before dropping
    /// the server to let the clients know that you intentionally disconnected. The `discon_msg`
    /// allows you to give a reason for the disconnect.
    pub fn disconnect<T: NetMsg>(&mut self, discon_msg: &T, cid: CId) -> io::Result<()> {
        if !self.cid_connected(cid) {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid CId."));
        }
        debug!("Disconnecting CId {}", cid);
        self.send_to(cid, discon_msg)?;
        self.disconnected.push_back((cid, Status::Disconnecting));
        Ok(())
    }

    /// Handles all available new connection attempts in a loop, calling the `hook` for each.
    ///
    /// The hook function should return a [`Response`] for weather to connect the client along with
    /// the respective message.
    ///
    /// Returns the number of handled connections.
    pub fn handle_new_cons(
        &mut self,
        hook: impl FnMut(CId, SocketAddr, C) -> Response<A, R>,
    ) -> u32 {
        self.connection.handle_pending(hook)
    }

    /// Handles all remaining disconnects.
    ///
    /// Returns the number of disconnects handled.
    pub fn handle_disconnects(&mut self, mut hook: impl FnMut(CId, Status)) -> u32 {
        // disconnect counts.
        let mut count = 0;

        while let Some((cid, status)) = self.disconnected.pop_front() {
            hook(cid, status);
            debug!("Removing CId {}", cid);
            // TODO: validate expect
            self.connection
                .remove_connection(cid)
                .expect("cid to be connected");
            count += 1;
        }
        count
    }

    /// Sends a message to the [`CId`] `cid`.
    pub fn send_to<M: NetMsg>(&mut self, cid: CId, msg: &M) -> io::Result<()> {
        self.connection.send_to(cid, msg)
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: NetMsg>(&mut self, msg: &T) -> io::Result<()> {
        for cid in self.cids().collect::<Vec<_>>() {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Sends a message to all [`CId`]s that match `spec`.
    pub fn send_spec<T: NetMsg>(&mut self, spec: CIdSpec, msg: &T) -> io::Result<()> {
        for cid in self
            .cids()
            .filter(|cid| spec.matches(*cid))
            .collect::<Vec<_>>()
        {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs) before calling this.
    ///
    /// ### Panics
    /// Panics if the type `M` was not registered.
    /// For a non-panicking version, see [try_recv()](Self::try_recv).
    pub fn recv<M: NetMsg>(&self) -> impl Iterator<Item = Message<M>> {
        self.msg_table.check_type::<M>().expect(
            "`recv` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv`",
        );
        let tid = TypeId::of::<M>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buf[m_type]
            .iter()
            .map(|m| m.get_typed::<M>().unwrap())
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs) before calling this.
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv<M: NetMsg>(&self) -> Option<impl Iterator<Item = Message<M>>> {
        let tid = TypeId::of::<M>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(
            self.msg_buf[m_type]
                .iter()
                .map(|m| m.get_typed::<M>().unwrap()),
        )
    }

    /// Gets an iterator for the messages of type `M` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs)
    ///
    /// ### Panics
    /// Panics if the type `M` was not registered.
    /// For a non-panicking version, see [try_recv_spec()](Self::try_recv_spec).
    pub fn recv_spec<M: NetMsg>(&self, spec: CIdSpec) -> impl Iterator<Item = Message<M>> + '_ {
        self.msg_table.check_type::<M>().expect(
            "`recv_spec` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv_spec`",
        );
        let tid = TypeId::of::<M>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buf[m_type]
            .iter()
            .filter(move |net_msg| spec.matches(net_msg.cid))
            .map(|net_msg| net_msg.get_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `M` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs)
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv_spec<M: NetMsg>(
        &self,
        spec: CIdSpec,
    ) -> Option<impl Iterator<Item = Message<M>> + '_> {
        let tid = TypeId::of::<M>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(
            self.msg_buf[m_type]
                .iter()
                .filter(move |net_msg| spec.matches(net_msg.cid))
                .map(|net_msg| net_msg.get_typed().unwrap()),
        )
    }

    /// Receives the messages from the connections. This is called in `server.tick()`.
    fn get_msgs(&mut self) {
        loop {
            match self.connection.recv_from() {
                Err(e) if e.kind() == WouldBlock => break,
                Err(e) => {
                    error!("Error receiving data: {}", e);
                }
                Ok((cid, header, msg)) => {
                    // TODO: handle special message types here
                    self.msg_buf[header.m_type].push(ErasedNetMsg {
                        cid,
                        order_num: header.order_num,
                        ack_num: header.sender_ack_num,
                        msg,
                    });
                }
            }
        }
    }

    /// Clears messages from the buffer.
    fn clear_msgs(&mut self) {
        for buff in self.msg_buf.iter_mut() {
            buff.clear();
        }
    }

    /// This handles everything that the server needs to do each frame.
    ///
    /// This includes:
    ///
    ///  - Clearing the message buffer. This gets rid of all the messages from last frame.
    ///  - (Re)sending messages that are needed for the reliability layer.
    ///  - Getting the messages for this frame.
    pub fn tick(&mut self) {
        self.clear_msgs();
        self.connection.send_ack_msgs();
        self.connection.send_pings();
        self.connection.resend_reliable();
        self.get_msgs();
    }

    /// Gets the address that the server is listening on.
    pub fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.connection.listen_addr()
    }

    /// An iterator of the [`CId`]s.
    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.connection.cids()
    }

    /// Returns whether the connection of the given [`CId`] is connected.
    pub fn cid_connected(&self, cid: CId) -> bool {
        self.connection.cid_connected(cid)
    }

    /// Returns whether a message of type `tid` can be sent.
    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.msg_table.valid_tid(tid)
    }

    /// The number of active connections. To ensure an accurate count, it is best to call this
    /// after calling [`handle_disconnects()`](Self::handle_disconnects).
    pub fn connection_count(&self) -> usize {
        self.connection.connection_count()
    }

    /// Gets the address of the given [`CId`].
    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.connection.addr_of(cid)
    }

    /// Gets the address of the given [`CId`].
    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.connection.cid_of(addr)
    }

    /// Gets the estimated round trip time (RTT) of the connection
    /// in microseconds (divide by 1000 for ms).
    ///
    /// Returns `None` iff `cid` is an invalid Connection ID.
    pub fn rtt(&self, cid: CId) -> Option<u32> {
        self.connection.rtt(cid)
    }
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Debug for Server<C, A, R, D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("listen_addr", &self.listen_addr())
            .field("connection_count", &self.connection_count())
            .finish()
    }
}
