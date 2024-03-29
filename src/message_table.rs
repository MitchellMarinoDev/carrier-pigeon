use crate::message_table::MsgRegError::TypeAlreadyRegistered;
use crate::net::{DeserFn, SerFn, Transport};
use crate::MId;
use hashbrown::HashMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use MsgRegError::NonUniqueIdentifier;

/// A type for collecting the parts needed to send a struct over the network.
///
/// IMPORTANT: The Message tables on all clients and the server **need** to have exactly the same
/// types registered **in the same order**. If this is not possible, use [`SortedMsgTable`].
#[derive(Clone, Default)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct MsgTable {
    table: Vec<(TypeId, Transport, SerFn, DeserFn)>,
}

/// A type for collecting the parts needed to send a struct over the network.
///
/// This is a variation of [`MsgTable`]. You should use this type only when you don't know the
/// order of registration. In place of a constant registration order, types must be registered
/// with a unique string identifier. The list is then sorted on this identifier when built.
///
/// If a type is registered with the same name, it will be ignored, therefore namespacing is
/// encouraged if you are allowing mods or external plugins to add networking types.
///
/// IMPORTANT: The Message tables on all clients and the server **need** to have exactly the
/// same types registered, although they do **not** need to be registered in the same order.
#[derive(Clone, Default)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct SortedMsgTable {
    table: Vec<(String, TypeId, Transport, SerFn, DeserFn)>,
}

/// The useful parts of the [`MsgTable`] (or [`SortedMsgTable`]).
///
/// You can build this by registering your types with a [`MsgTable`], then building it with
/// [`MsgTable::build()`].
#[derive(Clone)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct MsgTableParts {
    /// The mapping from TypeId to MessageId.
    pub tid_map: HashMap<TypeId, MId>,
    /// The transport associated with each message type.
    pub transports: Vec<Transport>,
    /// The serialization functions associated with each message type.
    pub ser: Vec<SerFn>,
    /// The deserialization functions associated with each message type.
    pub deser: Vec<DeserFn>,
}

pub const CONNECTION_TYPE_MID: MId = 0;
pub const RESPONSE_TYPE_MID: MId = 1;
pub const DISCONNECT_TYPE_MID: MId = 2;

impl MsgTable {
    /// Creates a new [`MsgTable`].
    pub fn new() -> Self {
        MsgTable::default()
    }

    /// Adds all registrations from `other` into this table.

    /// All errors are thrown before mutating self. If no errors are thrown, all entries are added;
    /// if an error is thrown, no entries are added.
    pub fn join(&mut self, other: &MsgTable) -> Result<(), MsgRegError> {
        // Validate
        if other
            .table
            .iter()
            .any(|(tid, _, _, _)| self.tid_registered(*tid))
        {
            return Err(TypeAlreadyRegistered);
        }

        // Join
        for entry in other.table.iter() {
            self.table.push(*entry);
        }
        Ok(())
    }

    /// If type `T` has been registered or not.
    pub fn is_registered<T>(&self) -> bool
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        let tid = TypeId::of::<T>();
        self.tid_registered(tid)
    }

    /// If the type with [`TypeId`] `tid` has been registered or not.
    pub fn tid_registered(&self, tid: TypeId) -> bool {
        self.table.iter().any(|(o_tid, _, _, _)| tid == *o_tid)
    }

    /// Registers a message type so that it can be sent over the network.
    pub fn register<T>(&mut self, transport: Transport) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        self.table.push(self.get_registration::<T>(transport)?);
        Ok(())
    }

    /// Builds the things needed for the registration.
    fn get_registration<T>(
        &self,
        transport: Transport,
    ) -> Result<(TypeId, Transport, SerFn, DeserFn), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Get the type.
        let tid = TypeId::of::<T>();

        // Check if it has been registered already.
        if self.tid_registered(tid) {
            return Err(TypeAlreadyRegistered);
        }

        // Get the serialize and deserialize functions
        let deser: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn Any + Send + Sync>)
                .map_err(|o| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("Deser Error: {}", o))
                })
        };
        let ser: SerFn = |m: &(dyn Any + Send + Sync)| {
            bincode::serialize(m.downcast_ref::<T>().unwrap()).map_err(|o| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Ser Error: {}", o))
            })
        };

        Ok((tid, transport, ser, deser))
    }

    /// Builds the [`MsgTable`] into useful parts.
    ///
    /// Consumes the Message table, and turns it into a [`MsgTableParts`].
    ///
    /// This should be called with the generic parameters:
    ///  - `C` is the connection message type.
    ///  - `R` is the response message type.
    ///  - `D` is the disconnect message type.
    ///
    /// The generic parameters should **not** be registered before hand.
    pub fn build<C, R, D>(self) -> Result<MsgTableParts, MsgRegError>
    where
        C: Any + Send + Sync + DeserializeOwned + Serialize,
        R: Any + Send + Sync + DeserializeOwned + Serialize,
        D: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Always prepend the Connection and Disconnect types first.
        // This gives them universal MIds.
        let con_discon_types = [
            self.get_registration::<C>(Transport::TCP)?,
            self.get_registration::<R>(Transport::TCP)?,
            self.get_registration::<D>(Transport::TCP)?,
        ];

        let mut tid_map = HashMap::with_capacity(self.table.len() + 3);
        let mut transports = Vec::with_capacity(self.table.len() + 3);
        let mut ser = Vec::with_capacity(self.table.len() + 3);
        let mut deser = Vec::with_capacity(self.table.len() + 3);

        // Add all types to parts. Connect type first, disconnect type second, all other types after
        for (idx, (tid, transport, s_fn, d_fn)) in con_discon_types
            .into_iter()
            .chain(self.table.into_iter())
            .enumerate()
        {
            tid_map.insert(tid, idx);
            transports.push(transport);
            ser.push(s_fn);
            deser.push(d_fn);
        }

        Ok(MsgTableParts {
            tid_map,
            transports,
            ser,
            deser,
        })
    }
}

impl SortedMsgTable {
    /// Creates a new [`SortedMsgTable`].
    pub fn new() -> Self {
        SortedMsgTable::default()
    }

    /// Adds all registrations from `other` into this table.
    ///
    /// All errors are thrown before mutating self. If no errors are thrown, all entries are added;
    /// if an error is thrown, no entries are added.
    pub fn join(&mut self, other: &SortedMsgTable) -> Result<(), MsgRegError> {
        // Validate
        if other
            .table
            .iter()
            .any(|(_, tid, _, _, _)| self.tid_registered(*tid))
        {
            return Err(TypeAlreadyRegistered);
        }

        if other
            .table
            .iter()
            .any(|(id, _, _, _, _)| self.identifier_registered(id))
        {
            return Err(NonUniqueIdentifier);
        }

        // Join
        for entry in other.table.iter() {
            self.table.push(entry.clone());
        }
        Ok(())
    }

    /// If type `T` has been registered or not.
    pub fn is_registered<T>(&self) -> bool
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        let tid = TypeId::of::<T>();
        self.tid_registered(tid)
    }

    /// If the type with [`TypeId`] `tid` has been registered or not.
    pub fn tid_registered(&self, tid: TypeId) -> bool {
        self.table.iter().any(|(_, o_tid, _, _, _)| tid == *o_tid)
    }

    /// If the type with [`TypeId`] `tid` has been registered or not.
    pub fn identifier_registered(&self, identifier: &str) -> bool {
        self.table.iter().any(|(id, _, _, _, _)| identifier == *id)
    }

    /// Registers a message type so that it can be sent over the network.
    pub fn register<T>(&mut self, transport: Transport, identifier: &str) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        self.table
            .push(self.get_registration::<T>(identifier.into(), transport)?);
        Ok(())
    }

    /// Builds the things needed for the registration.
    fn get_registration<T>(
        &self,
        identifier: String,
        transport: Transport,
    ) -> Result<(String, TypeId, Transport, SerFn, DeserFn), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Get the serialize and deserialize functions
        let deser: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn Any + Send + Sync>)
                .map_err(|o| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("Deser Error: {}", o))
                })
        };
        let ser: SerFn = |m: &(dyn Any + Send + Sync)| {
            bincode::serialize(m.downcast_ref::<T>().unwrap()).map_err(|o| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Ser Error: {}", o))
            })
        };

        // Check if the identifier has been registered already.
        if self.identifier_registered(&identifier) {
            return Err(NonUniqueIdentifier);
        }

        // Get the type.
        let tid = TypeId::of::<T>();

        // Check if it has been registered already.
        if self.tid_registered(tid) {
            return Err(TypeAlreadyRegistered);
        }

        Ok((identifier, tid, transport, ser, deser))
    }

    /// Builds the [`SortedMsgTable`] into useful parts.
    ///
    /// Consumes the Message table, and turns it into a [`MsgTableParts`].
    ///
    /// This should be called with the generic parameters:
    ///  - `C` is the connection message type.
    ///  - `R` is the response message type.
    ///  - `f` is the disconnect message type.
    ///
    /// The generic parameters should **not** be registered before hand.
    pub fn build<C, R, D>(mut self) -> Result<MsgTableParts, MsgRegError>
    where
        C: Any + Send + Sync + DeserializeOwned + Serialize,
        R: Any + Send + Sync + DeserializeOwned + Serialize,
        D: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Always prepend the Connection and Disconnect types first.
        // This gives them universal MIds.
        let con_discon_types = [
            self.get_registration::<C>("carrier-pigeon::connection".to_owned(), Transport::TCP)?,
            self.get_registration::<R>("carrier-pigeon::response".to_owned(), Transport::TCP)?,
            self.get_registration::<D>("carrier-pigeon::disconnect".to_owned(), Transport::TCP)?,
        ];

        // Sort by identifier string so that registration order doesn't matter.
        self.table
            .sort_by(|(id0, _, _, _, _), (id1, _, _, _, _)| id0.cmp(id1));

        let mut tid_map = HashMap::with_capacity(self.table.len() + 3);
        let mut transports = Vec::with_capacity(self.table.len() + 3);
        let mut ser = Vec::with_capacity(self.table.len() + 3);
        let mut deser = Vec::with_capacity(self.table.len() + 3);

        // Add all types to parts. Connect type first, disconnect type second, all other types after
        for (idx, (_identifier, tid, transport, s_fn, d_fn)) in con_discon_types
            .into_iter()
            .chain(self.table.into_iter())
            .enumerate()
        {
            tid_map.insert(tid, idx);
            transports.push(transport);
            ser.push(s_fn);
            deser.push(d_fn);
        }

        Ok(MsgTableParts {
            tid_map,
            transports,
            ser,
            deser,
        })
    }
}

impl MsgTableParts {
    /// Gets the number of registered `MId`s.
    pub fn mid_count(&self) -> usize {
        self.transports.len()
    }

    /// Checks if the [`MId`] `mid` is valid.
    pub fn valid_mid(&self, mid: MId) -> bool {
        mid <= self.mid_count()
    }

    /// Checks if the [`TypeId`] `tid` is registered.
    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.tid_map.contains_key(&tid)
    }
}

/// The possible errors when registering a type.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum MsgRegError {
    /// The type was already registered.
    TypeAlreadyRegistered,
    /// The identifier string was already used.
    NonUniqueIdentifier,
}

impl Display for MsgRegError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAlreadyRegistered => write!(f, "Type was already registered."),
            NonUniqueIdentifier => write!(f, "The identifier was not unique."),
        }
    }
}
