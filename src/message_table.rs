use crate::message_table::MsgRegError::TypeAlreadyRegistered;
use crate::net::{DeserFn, SerFn, Transport};
use crate::MId;
use hashbrown::HashMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use std::marker::PhantomData;
use MsgRegError::NonUniqueIdentifier;

/// A type for collecting the parts needed to send
/// a struct over the network.
///
/// IMPORTANT: The Message tables on all clients and
/// the server **need** to have exactly the same types
/// registered **in the same order**.
/// If this is not possible, use [`SortedMsgTable`].
#[derive(Clone)]
pub struct MsgTable {
    table: Vec<(TypeId, Transport, SerFn, DeserFn)>,
}

/// A type for collecting the parts needed to send a
/// struct over the network.
///
/// This is a variation of [`MsgTable`]. You should use this
/// type only when you don't know the order of registration.
/// In place of a constant registration order, types must be
/// registered with a unique string. The list is then sorted
/// on this string when built.
///
/// If a type is registered with the same name, it will be
/// ignored, therefore namespacing is encouraged if you are
/// allowing modders or external plugins to add networking
/// types.
///
/// IMPORTANT: The Message tables on all clients and the
/// server **need** to have exactly the same types
/// registered, although they do **not** need to be registered
/// in the same order.
#[derive(Clone)]
pub struct SortedMsgTable {
    table: Vec<(String, TypeId, Transport, SerFn, DeserFn)>,
}

/// The useful parts of the [`MsgTable`]
/// (or [`SortedMsgTable`]).
///
/// You can build this by registering your types
/// with a [`MsgTable`], then building it
/// with [`MsgTable::build()`].
#[derive(Clone)]
pub struct MsgTableParts<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    pub tid_map: HashMap<TypeId, MId>,
    pub transports: Vec<Transport>,
    pub ser: Vec<SerFn>,
    pub deser: Vec<DeserFn>,
    _pd: PhantomData<(C, R, D)>,
}

pub const CONNECTION_TYPE_MID: MId = 0;
pub const RESPONSE_TYPE_MID: MId = 1;
pub const DISCONNECT_TYPE_MID: MId = 2;

impl MsgTable {
    /// Creates a new [`MsgTable`].
    pub fn new() -> Self {
        MsgTable { table: vec![] }
    }

    /// Registers a message type so that it can be sent over the network.
    pub fn register<T>(&mut self, transport: Transport) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        let registration = self.get_registration::<T>(transport)?;
        self.table.push(registration);
        Ok(())
    }

    /// Registers a message type with custom serialization and deserialization logic.
    pub fn register_custom<T>(
        &mut self,
        transport: Transport,
        ser: SerFn,
        deser: DeserFn,
    ) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync,
    {
        // Get the type.
        let tid = TypeId::of::<T>();

        // Check if it has been registered already.
        if self.table.iter().any(|(t, _, _, _)| *t == tid) {
            return Err(TypeAlreadyRegistered);
        }

        let registration = (tid, transport, ser, deser);
        self.table.push(registration);
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
        if self.table.iter().any(|(t, _, _, _)| *t == tid) {
            return Err(TypeAlreadyRegistered);
        }

        // Get the serialize and deserialize functions
        let deser_fn: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn Any + Send + Sync>)
                .map_err(|o| io::Error::new(io::ErrorKind::InvalidData, format!("{}", o)))
        };
        let ser_fn: SerFn = |m: &(dyn Any + Send + Sync)| {
            bincode::serialize(m.downcast_ref::<T>().unwrap())
                .map_err(|o| io::Error::new(io::ErrorKind::InvalidData, format!("{}", o)))
        };

        Ok((tid, transport, ser_fn, deser_fn))
    }

    /// Builds the [`MsgTable`] into useful parts.
    ///
    /// Consumes the Message table, and turns it into a [`MsgTableParts`].
    ///
    /// This should be called with the generic parameters:
    ///  - C is the connection packet type.
    ///  - R is the response packet type.
    ///  - D is the disconnect packet type.
    ///
    /// The generic parameters should **not** be registered before hand.
    pub fn build<C, R, D>(self) -> Result<MsgTableParts<C, R, D>, MsgRegError>
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
            _pd: PhantomData,
        })
    }
}

impl SortedMsgTable {
    /// Creates a new [`SortedMsgTable`].
    pub fn new() -> Self {
        SortedMsgTable { table: vec![] }
    }

    /// Registers a message type so that it can be sent over the network.
    pub fn register<T>(&mut self, transport: Transport, identifier: &str) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        let registration = self.get_registration::<T>(transport, identifier.into())?;
        self.table.push(registration);
        Ok(())
    }

    /// Registers a message type with custom serialization and deserialization logic.
    pub fn register_custom<T>(
        &mut self,
        transport: Transport,
        identifier: &str,
        ser: SerFn,
        deser: DeserFn,
    ) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync,
    {
        let identifier = identifier.into();

        // Check if the identifier has been registered already.
        if self.table.iter().any(|(id, _, _, _, _)| *id == identifier) {
            return Err(NonUniqueIdentifier);
        }

        // Get the type.
        let tid = TypeId::of::<T>();

        // Check if it has been registered already.
        if self.table.iter().any(|(_, t, _, _, _)| *t == tid) {
            return Err(TypeAlreadyRegistered);
        }

        let registration = (identifier, tid, transport, ser, deser);
        self.table.push(registration);
        Ok(())
    }

    /// Builds the things needed for the registration.
    fn get_registration<T>(
        &self,
        transport: Transport,
        identifier: String,
    ) -> Result<(String, TypeId, Transport, SerFn, DeserFn), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Check if the identifier has been registered already.
        if self.table.iter().any(|(id, _, _, _, _)| *id == identifier) {
            return Err(NonUniqueIdentifier);
        }

        // Get the type.
        let tid = TypeId::of::<T>();

        // Check if it has been registered already.
        if self.table.iter().any(|(_, t, _, _, _)| *t == tid) {
            return Err(TypeAlreadyRegistered);
        }

        // Get the serialize and deserialize functions
        let deser_fn: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn Any + Send + Sync>)
                .map_err(|o| io::Error::new(io::ErrorKind::InvalidData, format!("{}", o)))
        };
        let ser_fn: SerFn = |m: &(dyn Any + Send + Sync)| {
            bincode::serialize(m.downcast_ref::<T>().unwrap())
                .map_err(|o| io::Error::new(io::ErrorKind::InvalidData, format!("{}", o)))
        };

        Ok((identifier, tid, transport, ser_fn, deser_fn))
    }

    /// Builds the [`SortedMsgTable`] into useful parts.
    ///
    /// Consumes the Message table, and turns it into a [`MsgTableParts`].
    ///
    /// This should be called with the generic parameters:
    ///  - C is the connection packet type.
    ///  - R is the response packet type.
    ///  - D is the disconnect packet type.
    ///
    /// The generic parameters should **not** be registered before hand.
    pub fn build<C, R, D>(mut self) -> Result<MsgTableParts<C, R, D>, MsgRegError>
    where
        C: Any + Send + Sync + DeserializeOwned + Serialize,
        R: Any + Send + Sync + DeserializeOwned + Serialize,
        D: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Always prepend the Connection and Disconnect types first.
        // This gives them universal MIds.
        let con_discon_types = [
            self.get_registration::<C>(Transport::TCP, "carrier-pigeon::connection".to_owned())?,
            self.get_registration::<R>(Transport::TCP, "carrier-pigeon::response".to_owned())?,
            self.get_registration::<D>(Transport::TCP, "carrier-pigeon::disconnect".to_owned())?,
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
            _pd: PhantomData,
        })
    }
}

impl<C, R, D> MsgTableParts<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    // TODO: use this function call in all appropriate senarios.
    pub fn mid_count(&self) -> usize {
        self.transports.len()
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
            TypeAlreadyRegistered => {
                write!(f, "Type was already registered.")
            }
            NonUniqueIdentifier => {
                write!(f, "The identifier was not unique.")
            }
        }
    }
}
