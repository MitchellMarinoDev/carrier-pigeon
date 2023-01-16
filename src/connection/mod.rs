pub mod client;
pub mod server;

use crate::net::MNum;
use crate::util::{DoubleHashMap, DoubleHashMapError};
use crate::CId;
use hashbrown::HashMap;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Contains the logic for mapping connection ids [`CId`]s to [`SocketAddr`]s.
///
/// Also manages a `addrs` which holds a sorted list of all the addresses that are
/// connected.
struct ConnectionList {
    cid_addr: DoubleHashMap<CId, SocketAddr>,
    /// A set of all the connected addresses.
    ///
    /// This is wrapped in multithreading types as it is used in the ServerTransport so that
    /// the transport doesnt waste time handling messages that arent from any of the connected
    /// clients.
    addrs: Arc<Mutex<BTreeSet<SocketAddr>>>,
}

impl ConnectionList {
    fn new() -> Self {
        ConnectionList {
            cid_addr: DoubleHashMap::new(),
            addrs: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    pub fn connect(&mut self, cid: CId, addr: SocketAddr) -> Result<(), DoubleHashMapError> {
        self.cid_addr.insert(cid, addr)?;
        {
            let mut addrs = self.addrs.lock().expect("failed to obtain lock");
            addrs.insert(addr);
        }
        Ok(())
    }

    pub fn disconnect(&mut self, cid: CId) -> bool {
        if let Some(addr) = self.cid_addr.remove(&cid) {
            let mut addrs = self.addrs.lock().expect("failed to obtain lock");
            assert!(
                addrs.remove(&addr),
                "`addrs` did not have an address that was in the double hash map"
            );
            return true;
        }
        false
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct SavedMsg {
    /// The payload of the message.
    payload: Arc<Vec<u8>>,
    /// The [`Instant`] that the message is sent.
    ///
    /// Used for knowing when to resend.
    sent: Instant,
}

impl SavedMsg {
    fn new(payload: Arc<Vec<u8>>) -> Self {
        SavedMsg {
            payload,
            sent: Instant::now(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NonAckedMsgs(HashMap<MNum, SavedMsg>);

impl NonAckedMsgs {
    fn new() -> Self {
        NonAckedMsgs(HashMap::with_capacity(0))
    }
}

impl Deref for NonAckedMsgs {
    type Target = HashMap<MNum, SavedMsg>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NonAckedMsgs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
