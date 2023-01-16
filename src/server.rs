use crate::connection::server::ServerConnection;
use crate::message_table::{MsgTable, CONNECTION_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{CId, CIdSpec, ErasedNetMsg, NetConfig, NetMsg, Status};
use crate::transport::server_std_udp::UdpServerTransport;
use log::*;
use std::any::{Any, TypeId};
use std::collections::VecDeque;
use std::io;
use std::io::ErrorKind::WouldBlock;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};

/// A server that manages connections to multiple clients.
///
/// Listens on a address and port, allowing for clients to connect. Newly connected clients will
/// be given a connection ID ([`CId`]) starting at `1` that is unique for the session.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Server {
    /// The configuration of the server.
    config: NetConfig,
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buf: Vec<Vec<ErasedNetMsg>>,

    /// Disconnected connections.
    disconnected: VecDeque<(CId, Status)>,
    /// The connection for this server.
    connection: ServerConnection<UdpServerTransport>,

    /// The [`MsgTable`] to use for sending messages.
    msg_table: MsgTable,
}

impl Server {
    /// Creates a new [`Server`].
    pub fn new(
        listen_addr: impl ToSocketAddrs,
        msg_table: MsgTable,
        config: NetConfig,
    ) -> io::Result<Self> {
        let connection = ServerConnection::new(msg_table.clone(), listen_addr)?;

        let mid_count = msg_table.tid_map.len();
        let msg_buf = (0..mid_count).map(|_| vec![]).collect();

        Ok(Server {
            config,
            msg_buf,
            disconnected: VecDeque::new(),
            connection,
            msg_table,
        })
    }

    /// Gets the config of the server.
    pub fn config(&self) -> &NetConfig {
        &self.config
    }

    /// Disconnects from the given `cid`. You should always disconnect all clients before dropping
    /// the server to let the clients know that you intentionally disconnected. The `discon_msg`
    /// allows you to give a reason for the disconnect.
    pub fn disconnect<T: Any + Send + Sync>(&mut self, discon_msg: &T, cid: CId) -> io::Result<()> {
        if !self.cid_connected(cid) {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid CId."));
        }
        debug!("Disconnecting CId {}", cid);
        self.send_to(cid, discon_msg)?;
        self.disconnected.push_back((cid, Status::Closed));
        Ok(())
    }

    /// Handles all available new connection attempts in a loop, calling the `hook` for each.
    ///
    /// The hook function should return `(should_accept, response_msg)`.
    ///
    /// Returns the number of handled connections.
    ///
    /// ### Panics
    /// Panics if the generic parameters `C` and `R` are not the same `C` and `R`
    /// that you passed into [`MsgTableBuilder::build`](crate::MsgTableBuilder::build).
    pub fn handle_new_cons<C: Any + Send + Sync, R: Any + Send + Sync>(
        &mut self,
        mut hook: impl FnMut(CId, SocketAddr, C) -> (bool, R),
    ) -> u32 {
        // verify that `C` and `R` are the right type.
        let c_tid = TypeId::of::<C>();
        let r_tid = TypeId::of::<R>();
        if self.msg_table.tid_map.get(&c_tid) != Some(&CONNECTION_TYPE_MID)
            || self.msg_table.tid_map.get(&r_tid) != Some(&RESPONSE_TYPE_MID)
        {
            panic!(
                "generic parameters `C` and `R` need to match the generic parameters \
            that you passed into `MsgTableBuilder::build()`"
            );
        }

        self.connection
            .handle_pending(hook)
            .expect("already checked generic parameters")
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
    pub fn send_to<M: Any + Send + Sync>(&self, cid: CId, msg: &M) -> io::Result<()> {
        self.connection.send_to(cid, msg)
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: Any + Send + Sync>(&self, msg: &T) -> io::Result<()> {
        for cid in self.cids().collect::<Vec<_>>() {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Sends a message to all [`CId`]s that match `spec`.
    pub fn send_spec<T: Any + Send + Sync>(&self, spec: CIdSpec, msg: &T) -> io::Result<()> {
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
    pub fn recv<M: Any + Send + Sync>(&self) -> impl Iterator<Item = NetMsg<M>> {
        self.msg_table.check_type::<M>().expect(
            "`recv` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv`",
        );
        let tid = TypeId::of::<M>();
        let mid = self.msg_table.tid_map[&tid];

        self.msg_buf[mid]
            .iter()
            .map(|m| m.get_typed::<M>().unwrap())
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs) before calling this.
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv<M: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = NetMsg<M>>> {
        let tid = TypeId::of::<M>();
        let mid = *self.msg_table.tid_map.get(&tid)?;

        Some(
            self.msg_buf[mid]
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
    pub fn recv_spec<M: Any + Send + Sync>(
        &self,
        spec: CIdSpec,
    ) -> impl Iterator<Item = NetMsg<M>> + '_ {
        self.msg_table.check_type::<M>().expect(
            "`recv_spec` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv_spec`",
        );
        let tid = TypeId::of::<M>();
        let mid = self.msg_table.tid_map[&tid];

        self.msg_buf[mid]
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
    pub fn try_recv_spec<M: Any + Send + Sync>(
        &self,
        spec: CIdSpec,
    ) -> Option<impl Iterator<Item = NetMsg<M>> + '_> {
        let tid = TypeId::of::<M>();
        let mid = *self.msg_table.tid_map.get(&tid)?;

        Some(
            self.msg_buf[mid]
                .iter()
                .filter(move |net_msg| spec.matches(net_msg.cid))
                .map(|net_msg| net_msg.get_typed().unwrap()),
        )
    }

    /// Receives the messages from the connections. This should be done before calling `recv<T>()`.
    ///
    /// When done in a game loop, you should call `clear_msgs()`, then `get_msgs()` before default
    /// time. This will clear the messages between frames.
    pub fn get_msgs(&mut self) -> u32 {
        let mut count = 0;

        loop {
            match self.connection.recv_from() {
                Err(e) if e.kind() == WouldBlock => break,
                Err(e) => {
                    error!("IO error occurred while receiving data. {}", e);
                }
                Ok((cid, header, msg)) => {
                    // TODO: handle special message types here
                    count += 1;
                    self.msg_buf[header.mid].push(ErasedNetMsg { cid, msg });
                }
            }
        }
        count
    }

    /// Clears messages from the buffer.
    pub fn clear_msgs(&mut self) {
        for buff in self.msg_buf.iter_mut() {
            buff.clear();
        }
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
}
