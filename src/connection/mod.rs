mod ack_system;
mod ordering_system;
pub(crate) mod ping_system;
pub(crate) mod reliable;
#[cfg(test)]
mod test_connection;

use crate::messages::NetMsg;
use crate::util::DoubleHashMap;
use crate::CId;
use std::collections::VecDeque;
use std::io::Error;
use std::net::SocketAddr;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum ConnectionListError {
    /// The [`SocketAddress`] was already connected.
    AlreadyConnected,
    /// The [`CId`] was not connected.
    NotConnected,
}

/// An enum representing the possible disconnection event types.
pub enum DisconnectionType<D: NetMsg> {
    /// The connection was dropped without sending a disconnection message.
    Dropped(Error),
    /// The peer disconnected by sending a disconnection message.
    Disconnected(D),
    /// The server disconnected the peer with message.
    ServerDisconnected(D),
}

/// An enum representing the possible disconnection events
pub struct DisconnectionEvent<D: NetMsg> {
    /// The [`CId`] that the event it for.
    pub cid: CId,
    /// The type of disconnection event.
    pub disconnection_type: DisconnectionType<D>,
}

impl<D: NetMsg> DisconnectionEvent<D> {
    /// Creates a new [`Dropped`](DisconnectionType::Dropped)
    /// type [`DisconnectionEvent`] with the given `cid` and `err`.
    pub fn dropped(cid: CId, err: Error) -> Self {
        DisconnectionEvent {
            cid,
            disconnection_type: DisconnectionType::Dropped(err),
        }
    }

    /// Creates a new [`Disconnected`](DisconnectionType::Disconnected)
    /// type [`DisconnectionEvent`] with the given `cid` and `disconnect_msg`.
    pub fn disconnected(cid: CId, disconnect_msg: D) -> Self {
        DisconnectionEvent {
            cid,
            disconnection_type: DisconnectionType::Disconnected(disconnect_msg),
        }
    }

    /// Creates a new [`ServerDisconnected`](DisconnectionType::ServerDisconnected)
    /// type [`DisconnectionEvent`] with the given `cid` and `disconnect_msg`.
    pub fn server_disconnected(cid: CId, disconnect_msg: D) -> Self {
        DisconnectionEvent {
            cid,
            disconnection_type: DisconnectionType::ServerDisconnected(disconnect_msg),
        }
    }
}

/// Contains the logic for mapping connection ids [`CId`]s to [`SocketAddr`]s.
///
/// Also manages a `addrs` which holds a sorted list of all the addresses that are
/// connected.
pub(crate) struct ConnectionList<C: NetMsg> {
    /// The current [`CId`]. The id to assign to new connections (then increment).
    current_cid: CId,
    /// The mapping of [`CId`]s to [`SocketAddr`]s and back.
    cid_addr: DoubleHashMap<CId, SocketAddr>,
    /// A que that keeps track of new unhandled connections.
    // TODO: I dont think a cid needs to be assigned until/unless the connection is accepted.
    pending_connections: VecDeque<(CId, SocketAddr, C)>,
}

impl<C: NetMsg> ConnectionList<C> {
    pub fn new() -> Self {
        ConnectionList {
            current_cid: 1,
            cid_addr: DoubleHashMap::new(),
            pending_connections: VecDeque::new(),
        }
    }

    /// Adds a new pending connection.
    ///
    /// This assigns the address a [`CId`], but does not consider it a connected client.
    /// Therefore, calling [`cid_connected`](Self::cid_connected) and
    /// [`addr_connected`](Self::addr_connected) will return false.
    ///
    /// Returns the [`CId`] that was assigned.
    ///
    /// Returns an error if the address is already connected.
    pub fn new_pending(
        &mut self,
        addr: SocketAddr,
        connection_msg: C,
    ) -> Result<CId, ConnectionListError> {
        let cid = self.current_cid;
        self.current_cid += 1;
        self.pending_connections
            .push_back((cid, addr, connection_msg));
        Ok(cid)
    }

    /// Gets the next pending connection if there is one.
    pub fn get_pending(&mut self) -> Option<(CId, SocketAddr, C)> {
        self.pending_connections.pop_front()
    }

    /// Handles adding a new connection.
    ///
    /// ### Errors
    /// Returns an error if `cid`, or `addr` are already connected.
    pub fn new_connection(&mut self, cid: CId, addr: SocketAddr) -> Result<(), ConnectionListError> {
        self.cid_addr
            .insert(cid, addr)
            .map_err(|_| ConnectionListError::AlreadyConnected)?;
        Ok(())
    }

    pub fn remove_connection(&mut self, cid: CId) -> Result<(), ConnectionListError> {
        if !self.cid_connected(cid) {
            return Err(ConnectionListError::NotConnected);
        }
        let _addr = self.cid_addr.remove(&cid).expect("cid should be connected");
        Ok(())
    }

    pub fn connection_count(&self) -> usize {
        self.cid_addr.len()
    }

    pub fn cid_connected(&self, cid: CId) -> bool {
        self.cid_addr.contains_key(&cid)
    }

    pub fn addr_connected(&self, addr: SocketAddr) -> bool {
        self.cid_addr.contains_value(&addr)
    }

    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.cid_addr.get(&cid).copied()
    }

    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.cid_addr.get_backward(&addr).copied()
    }

    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.cid_addr.keys().copied()
    }

    pub fn addrs(&self) -> impl Iterator<Item = SocketAddr> + '_ {
        self.cid_addr.values().copied()
    }

    pub fn pairs(&self) -> impl Iterator<Item = (CId, SocketAddr)> + '_ {
        self.cid_addr.pairs().map(|(&cid, &addr)| (cid, addr))
    }
}
