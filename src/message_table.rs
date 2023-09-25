use crate::message_table::MsgRegError::TypeAlreadyRegistered;
use crate::messages::{AckMsg, NetMsg, PingMsg};
use crate::net::Guarantees;
use crate::{MType, Response};
use hashbrown::HashMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{type_name, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use MsgRegError::NonUniqueIdentifier;

/// The function used to deserialize a message.
///
/// fn(&[[u8]]) -> [`io::Result`]<[`Box`]<dyn [`Any`] + [`Send`] + [`Sync`]>>
pub type DeserFn = fn(&[u8]) -> io::Result<Box<dyn NetMsg>>;
/// The function used to serialize a message.
///
/// fn(&(dyn [`Any`] + [`Send`] + [`Sync`]), &mut [`Vec`]<[`u8`]>) -> [`io::Result`]<()>
pub type SerFn = fn(&(dyn NetMsg), &mut Vec<u8>) -> io::Result<()>;

/// A registration in the [`MsgTableBuilder`].
#[derive(Copy, Clone)]
struct Registration {
    tid: TypeId,
    guarantees: Guarantees,
    ser: SerFn,
    deser: DeserFn,
}

/// A type for building a [`MsgTable`].
///
/// This helps get the parts needed to send a struct over the network
#[derive(Clone, Default)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct MsgTableBuilder {
    /// The registrations where the user guarantees constant registration order.
    ordered: Vec<Registration>,
    /// The registrations where the user guarantees constant message identifiers.
    sorted: Vec<(String, Registration)>,
}

/// The inner structure of the [`MsgTable`].
pub struct MsgTableInner {
    /// The mapping from TypeId to MessageId.
    pub tid_map: HashMap<TypeId, MType>,
    /// The transport associated with each message type.
    pub guarantees: Vec<Guarantees>,
    /// The serialization functions associated with each message type.
    pub ser: Vec<SerFn>,
    /// The deserialization functions associated with each message type.
    pub deser: Vec<DeserFn>,
}

/// The table mapping [`TypeId`]s to [`MType`](crate::MType)s and [`Guarantees`].
///
/// You can build this by registering your types with a [`MsgTableBuilder`] or [`SortedMsgTableBuilder`], then building it with
/// [`MsgTableBuilder::build()`] or [`SortedMsgTableBuilder::build()`].
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct MsgTable<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    /// The message type data wrapped in an [`Arc`].
    inner: Arc<MsgTableInner>,
    _pd: PhantomData<(C, A, R, D)>,
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Clone for MsgTable<C, A, R, D> {
    fn clone(&self) -> Self {
        MsgTable {
            inner: self.inner.clone(),
            _pd: PhantomData,
        }
    }
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Deref for MsgTable<C, A, R, D> {
    type Target = MsgTableInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub const CONNECTION_M_TYPE: MType = 0;
pub const RESPONSE_M_TYPE: MType = 1;
pub const DISCONNECT_M_TYPE: MType = 2;
pub const ACK_M_TYPE: MType = 3;
pub const PING_M_TYPE: MType = 4;
pub const SPECIAL_M_TYPE_COUNT: usize = 5;

impl MsgTableBuilder {
    /// Creates a new [`MsgTableBuilder`].
    pub fn new() -> Self {
        MsgTableBuilder::default()
    }

    /// Adds all registrations from `other` into this table.
    ///
    /// All errors are thrown before mutating self. If no errors are thrown, all entries are added;
    /// if an error is thrown, no entries are added.
    pub fn join(&mut self, other: &MsgTableBuilder) -> Result<(), MsgRegError> {
        // validate
        // check for unique [`TypeId`]s
        for tid in other
            .ordered
            .iter()
            .map(|r| r.tid)
            .chain(other.sorted.iter().map(|(_id, r)| r.tid))
        {
            if self.tid_registered(tid) {
                return Err(TypeAlreadyRegistered(tid));
            }
        }
        // check for unique registration identifiers
        for identifier in other.sorted.iter().map(|(id, _r)| id) {
            if self.identifier_registered(identifier) {
                return Err(NonUniqueIdentifier(identifier.clone()));
            }
        }

        // Join
        self.ordered.extend(other.ordered.iter().cloned());
        self.sorted.extend(other.sorted.iter().cloned());
        Ok(())
    }

    /// If type `T` has been registered or not.
    pub fn is_registered<T>(&self) -> bool
        where
            T: NetMsg,
    {
        let tid = TypeId::of::<T>();
        self.tid_registered(tid)
    }

    /// If the type with [`TypeId`] `tid` has been registered or not.
    pub fn tid_registered(&self, tid: TypeId) -> bool {
        self.ordered.iter().any(|r| r.tid == tid) || self.sorted.iter().any(|(_id, r)| r.tid == tid)
    }

    /// If the message with `identifier` has been registered or not.
    pub fn identifier_registered(&self, identifier: &str) -> bool {
        self.sorted.iter().any(|(id, _r)| id == identifier)
    }

    /// Registers a message type so that it can be sent over the network.
    ///
    /// You must guarantee constant registration order for all messages
    /// between all clients and the server. If you cannot make this guarantee,
    /// use the [`register_sorted`] method. That method does not require a constant
    /// registration order, but instead requires a unique string identifier to be
    /// registered with each message.
    pub fn register_in_order<T: NetMsg + Serialize + DeserializeOwned>(
        &mut self,
        guarantees: Guarantees,
    ) -> Result<(), MsgRegError>
        where
            T: NetMsg,
    {
        self.ordered
            .push(self.get_ordered_registration::<T>(guarantees)?);
        Ok(())
    }

    /// Registers a message type so that it can be sent over the network.
    ///
    /// You must guarantee that the identifier is unique for all messages you register.
    /// You must also guarantee that the same messages are registered with the same
    /// identifier on all clients and the server.
    /// If you cannot make this guarantee, use the [`register_ordered`] method. That method
    /// requires constant registration order, but does not require a unique string identifier.
    pub fn register_sorted<T>(
        &mut self,
        guarantees: Guarantees,
        identifier: impl ToString,
    ) -> Result<(), MsgRegError>
        where
            T: NetMsg + DeserializeOwned + Serialize,
    {
        self.sorted
            .push(self.get_sorted_registration::<T>(guarantees, identifier)?);
        Ok(())
    }

    /// Builds the things needed for an ordered registration.
    fn get_ordered_registration<T>(
        &self,
        guarantees: Guarantees,
    ) -> Result<Registration, MsgRegError>
        where
            T: NetMsg + DeserializeOwned + Serialize,
    {
        // Check if it has been registered already.
        let tid = TypeId::of::<T>();
        if self.tid_registered(tid) {
            return Err(TypeAlreadyRegistered(tid));
        }

        // Get the serialize and deserialize functions
        let deser: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn NetMsg>)
                .map_err(|o| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("deserialization error: {}", o),
                    )
                })
        };
        let ser: SerFn = |m: &(dyn NetMsg), buf| {
            bincode::serialize_into(
                buf,
                m.downcast_ref::<T>()
                    .expect("wrong type passed to the message serialization function"),
            )
                .map_err(|o| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("serialization error: {}", o),
                    )
                })
        };

        Ok(Registration {
            tid,
            guarantees,
            ser,
            deser,
        })
    }

    /// Builds the things needed for a sorted registration.
    fn get_sorted_registration<T>(
        &self,
        guarantees: Guarantees,
        identifier: impl ToString,
    ) -> Result<(String, Registration), MsgRegError>
        where
            T: NetMsg + Serialize + DeserializeOwned,
    {
        let identifier = identifier.to_string();
        // Check if the identifier has been registered already.
        if self.identifier_registered(&identifier) {
            return Err(NonUniqueIdentifier(identifier));
        }

        let registration = self.get_ordered_registration::<T>(guarantees)?;

        Ok((identifier, registration))
    }

    /// Builds the [`MsgTable`].
    ///
    /// Consumes the [`MsgTableBuilder`], and turns it into a [`MsgTable`].
    ///
    /// This should be called with the generic parameters:
    ///  - `C` is the connection message type.
    ///  - `A` is the accepted message type.
    ///  - `R` is the rejected message type.
    ///  - `D` is the disconnect message type.
    ///
    /// The generic parameters should **not** be registered before hand.
    ///
    /// This fails iff the generic parameters have already been registered.
    pub fn build<C, A, R, D>(mut self) -> Result<MsgTable<C, A, R, D>, MsgRegError>
        where
            C: NetMsg + Serialize + DeserializeOwned,
            A: NetMsg + Serialize + DeserializeOwned,
            R: NetMsg + Serialize + DeserializeOwned,
            D: NetMsg + Serialize + DeserializeOwned,
    {
        // Always prepend the Connection and Disconnect types first.
        // This gives them universal MTypes.
        let con_discon_types = [
            self.get_ordered_registration::<C>(Guarantees::Reliable)?,
            self.get_ordered_registration::<Response<A, R>>(Guarantees::Reliable)?,
            self.get_ordered_registration::<D>(Guarantees::Reliable)?,
            self.get_ordered_registration::<AckMsg>(Guarantees::Unreliable)
                .expect("failed to create registration for AckMsg"),
            self.get_ordered_registration::<PingMsg>(Guarantees::Unreliable)
                .expect("failed to create registration for AckMsg"),
        ];

        let registration_count = self.ordered.len() + self.sorted.len() + SPECIAL_M_TYPE_COUNT;
        let mut tid_map = HashMap::with_capacity(registration_count);
        let mut guarantees = Vec::with_capacity(registration_count);
        let mut ser = Vec::with_capacity(registration_count);
        let mut deser = Vec::with_capacity(registration_count);

        // Sort by identifier string so that registration order doesn't matter.
        self.sorted.sort_by(|(id0, _r0), (id1, _r1)| id0.cmp(id1));

        // Add all types to parts.
        // Connection type first, Response type second, Disconnection type third
        // then all user defined messages.
        for (idx, registration) in con_discon_types
            .into_iter()
            .chain(self.ordered.into_iter())
            .chain(self.sorted.into_iter().map(|(_id, r)| r))
            .enumerate()
        {
            tid_map.insert(registration.tid, idx);
            guarantees.push(registration.guarantees);
            ser.push(registration.ser);
            deser.push(registration.deser);
        }

        Ok(MsgTable {
            inner: Arc::new(MsgTableInner {
                tid_map,
                guarantees,
                ser,
                deser,
            }),
            _pd: PhantomData,
        })
    }
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> MsgTable<C, A, R, D> {
    /// Gets the number of registered [`MType`](crate::MType)s.
    #[inline]
    pub fn mtype_count(&self) -> usize {
        self.guarantees.len()
    }

    /// Checks if the [`MType`](crate::MType) `m_type` is valid.
    #[inline]
    pub fn valid_m_type(&self, m_type: MType) -> bool {
        m_type <= self.mtype_count()
    }

    /// Checks if the [`TypeId`] `tid` is registered.
    #[inline]
    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.tid_map.contains_key(&tid)
    }

    /// Checks if the [`TypeId`] `tid` is registered. If it is not, it returns an error.
    ///
    /// If you have the type as a generic, use [`check_type`](Self::check_type) instead. It gives
    /// a better error message.
    pub fn check_tid(&self, tid: TypeId) -> io::Result<()> {
        if !self.valid_tid(tid) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Type ({:?}) not registered as a message.", tid),
            ));
        }
        Ok(())
    }

    /// Checks if the type `T` is registered. If it is not, it returns an error.
    pub fn check_type<T: NetMsg>(&self) -> io::Result<()> {
        let tid = TypeId::of::<T>();
        if !self.valid_tid(tid) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Type ({}) not registered as a message.", type_name::<T>()),
            ));
        }
        Ok(())
    }

    pub fn check_m_type(&self, m_type: MType) -> io::Result<()> {
        if !self.valid_m_type(m_type) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Message specified MType: {}, but the maximum MType is {}.",
                    m_type,
                    self.mtype_count()
                ),
            ));
        }
        Ok(())
    }
}

/// The possible errors when registering a type.
#[derive(Eq, PartialEq, Clone, Debug)]
pub enum MsgRegError {
    /// The type was already registered.
    TypeAlreadyRegistered(TypeId),
    /// The identifier string was already used.
    NonUniqueIdentifier(String),
}

impl Display for MsgRegError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAlreadyRegistered(tid) => write!(f, "Type ({:?}) was already registered.", tid),
            NonUniqueIdentifier(identifier) => {
                write!(f, "The identifier ({}) was not unique.", identifier)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    struct Connection;

    #[derive(Serialize, Deserialize, Debug)]
    struct Accepted;

    #[derive(Serialize, Deserialize, Debug)]
    struct Rejected;

    #[derive(Serialize, Deserialize, Debug)]
    struct Disconnect;

    #[derive(Serialize, Deserialize, Debug)]
    struct ReliableMsg;

    #[derive(Serialize, Deserialize, Debug)]
    struct UnreliableMsg;

    /// Tests the [`TypeAlreadyRegistered`], and [`NonUniqueIdentifier`]
    /// errors for both MsgTable variants.
    #[test]
    fn errors() {
        let mut builder = MsgTableBuilder::new();
        builder
            .register_in_order::<ReliableMsg>(Guarantees::Reliable)
            .unwrap();
        assert_eq!(
            builder.register_in_order::<ReliableMsg>(Guarantees::Reliable),
            Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>()))
        );

        let mut table = MsgTableBuilder::new();
        table
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "test::ReliableMsg")
            .unwrap();
        assert_eq!(
            table.register_sorted::<ReliableMsg>(Guarantees::Reliable, "test::ReliableMsg2"),
            Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>()))
        );

        let mut table = MsgTableBuilder::new();
        table
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "test::ReliableMsg")
            .unwrap();
        assert_eq!(
            table.register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "test::ReliableMsg"),
            Err(NonUniqueIdentifier("test::ReliableMsg".to_owned()))
        );
    }

    /// Tests [`MsgTableParts`] generation.
    #[test]
    fn parts_gen() {
        let mut table = MsgTableBuilder::new();
        table
            .register_in_order::<ReliableMsg>(Guarantees::Reliable)
            .unwrap();
        table
            .register_in_order::<UnreliableMsg>(Guarantees::Unreliable)
            .unwrap();

        // Expected result:
        // TODO: extract this to common fn
        let mut expected_tid_map = HashMap::new();
        expected_tid_map.insert(TypeId::of::<Connection>(), 0);
        expected_tid_map.insert(TypeId::of::<Response<Accepted, Rejected>>(), 1);
        expected_tid_map.insert(TypeId::of::<Disconnect>(), 2);
        expected_tid_map.insert(TypeId::of::<AckMsg>(), 3);
        expected_tid_map.insert(TypeId::of::<PingMsg>(), 4);
        expected_tid_map.insert(TypeId::of::<ReliableMsg>(), 5);
        expected_tid_map.insert(TypeId::of::<UnreliableMsg>(), 6);

        let guarantees = vec![
            Guarantees::Reliable,   // Connection
            Guarantees::Reliable,   // Response
            Guarantees::Reliable,   // Disconnect
            Guarantees::Unreliable, // AckMsg
            Guarantees::Unreliable, // PingMsg
            Guarantees::Reliable,   // ReliableMsg
            Guarantees::Unreliable, // UnreliableMsg
        ];

        let parts = table
            .build::<Connection, Accepted, Rejected, Disconnect>()
            .unwrap();

        // Make sure the tid_map and transports generated correctly.
        assert_eq!(parts.tid_map, expected_tid_map);
        assert_eq!(parts.guarantees, guarantees);
        // Can't check ser and deser functions.
        assert_eq!(parts.ser.len(), 7);
        assert_eq!(parts.deser.len(), 7);
    }

    /// Tests [`MsgTableParts`] generation.
    #[test]
    fn parts_gen_sorted() {
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
            .unwrap();
        builder1
            .register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "tests::UnreliableMsg")
            .unwrap();

        // Different order.
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "tests::UnreliableMsg")
            .unwrap();
        builder2
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
            .unwrap();

        // Expected result:
        let mut expected_tid_map = HashMap::new();
        expected_tid_map.insert(TypeId::of::<Connection>(), 0);
        expected_tid_map.insert(TypeId::of::<Response<Accepted, Rejected>>(), 1);
        expected_tid_map.insert(TypeId::of::<Disconnect>(), 2);
        expected_tid_map.insert(TypeId::of::<AckMsg>(), 3);
        expected_tid_map.insert(TypeId::of::<PingMsg>(), 4);
        expected_tid_map.insert(TypeId::of::<ReliableMsg>(), 5);
        expected_tid_map.insert(TypeId::of::<UnreliableMsg>(), 6);

        let expected_guarantees = vec![
            Guarantees::Reliable,   // Connection
            Guarantees::Reliable,   // Response
            Guarantees::Reliable,   // Disconnect
            Guarantees::Unreliable, // AckMsg
            Guarantees::Unreliable, // PingMsg
            Guarantees::Reliable,   // ReliableMsg
            Guarantees::Unreliable, // UnreliableMsg
        ];

        let table1 = builder1
            .build::<Connection, Accepted, Rejected, Disconnect>()
            .unwrap();
        let table2 = builder2
            .build::<Connection, Accepted, Rejected, Disconnect>()
            .unwrap();

        // Make sure the tid_map and transports generated correctly.
        assert_eq!(table1.tid_map, expected_tid_map);
        assert_eq!(table1.guarantees, expected_guarantees);
        // Can't check ser and deser functions.
        assert_eq!(table1.ser.len(), 7);
        assert_eq!(table1.deser.len(), 7);

        // Make sure the tables generated the same.
        assert_eq!(table1.tid_map, table2.tid_map);
        assert_eq!(table1.guarantees, table2.guarantees);
    }

    /// Tests [`MsgTable`] joining.
    #[test]
    fn join() {
        // MsgTable
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_in_order::<ReliableMsg>(Guarantees::Reliable)
            .unwrap();
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_in_order::<UnreliableMsg>(Guarantees::Unreliable)
            .unwrap();

        builder1.join(&builder2).unwrap();

        // Expected result:
        let mut expected_tid_map = HashMap::new();
        expected_tid_map.insert(TypeId::of::<Connection>(), 0);
        expected_tid_map.insert(TypeId::of::<Response<Accepted, Rejected>>(), 1);
        expected_tid_map.insert(TypeId::of::<Disconnect>(), 2);
        expected_tid_map.insert(TypeId::of::<AckMsg>(), 3);
        expected_tid_map.insert(TypeId::of::<PingMsg>(), 4);
        expected_tid_map.insert(TypeId::of::<ReliableMsg>(), 5);
        expected_tid_map.insert(TypeId::of::<UnreliableMsg>(), 6);

        let expected_guarantees = vec![
            Guarantees::Reliable,   // Connection
            Guarantees::Reliable,   // Response
            Guarantees::Reliable,   // Disconnect
            Guarantees::Unreliable, // AckMsg
            Guarantees::Unreliable, // PingMsg
            Guarantees::Reliable,   // ReliableMsg
            Guarantees::Unreliable, // UnreliableMsg
        ];

        let parts = builder1
            .build::<Connection, Accepted, Rejected, Disconnect>()
            .unwrap();

        // Make sure the tid_map and transports generated correctly.
        assert_eq!(parts.tid_map, expected_tid_map);
        assert_eq!(parts.guarantees, expected_guarantees);
        // Can't check ser and deser functions.
        assert_eq!(parts.ser.len(), 7);
        assert_eq!(parts.deser.len(), 7);
    }

    /// Tests [`SortedMsgTable`] joining.
    #[test]
    fn join_sorted() {
        // MsgTable
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
            .unwrap();
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "tests::UnreliableMsg")
            .unwrap();

        builder1.join(&builder2).unwrap();

        // Expected result:
        let mut expected_tid_map = HashMap::new();
        expected_tid_map.insert(TypeId::of::<Connection>(), 0);
        expected_tid_map.insert(TypeId::of::<Response<Accepted, Rejected>>(), 1);
        expected_tid_map.insert(TypeId::of::<Disconnect>(), 2);
        expected_tid_map.insert(TypeId::of::<AckMsg>(), 3);
        expected_tid_map.insert(TypeId::of::<PingMsg>(), 4);
        expected_tid_map.insert(TypeId::of::<ReliableMsg>(), 5);
        expected_tid_map.insert(TypeId::of::<UnreliableMsg>(), 6);

        let expected_guarantees = vec![
            Guarantees::Reliable,   // Connection
            Guarantees::Reliable,   // Response
            Guarantees::Reliable,   // Disconnect
            Guarantees::Unreliable, // AckMsg
            Guarantees::Unreliable, // PingMsg
            Guarantees::Reliable,   // ReliableMsg
            Guarantees::Unreliable, // UnreliableMsg
        ];

        let table = builder1
            .build::<Connection, Accepted, Rejected, Disconnect>()
            .unwrap();

        // Make sure the tid_map and transports generated correctly.
        assert_eq!(table.tid_map, expected_tid_map);
        assert_eq!(table.guarantees, expected_guarantees);
        // Can't check ser and deser functions.
        assert_eq!(table.ser.len(), 7);
        assert_eq!(table.deser.len(), 7);
    }

    /// Tests [`MsgTable`] joining.
    #[test]
    fn join_error() {
        // ordered
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_in_order::<ReliableMsg>(Guarantees::Reliable)
            .unwrap();
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_in_order::<ReliableMsg>(Guarantees::Unreliable)
            .unwrap();

        assert_eq!(
            builder1.join(&builder2),
            Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>()))
        );

        // sorted
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
            .unwrap();
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_sorted::<ReliableMsg>(Guarantees::Unreliable, "tests::DifferentMsg")
            .unwrap();

        assert_eq!(
            builder1.join(&builder2),
            Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>()))
        );

        // sorted
        let mut builder1 = MsgTableBuilder::new();
        builder1
            .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
            .unwrap();
        let mut builder2 = MsgTableBuilder::new();
        builder2
            .register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "tests::ReliableMsg")
            .unwrap();

        assert_eq!(
            builder1.join(&builder2).unwrap_err(),
            NonUniqueIdentifier("tests::ReliableMsg".to_owned())
        );
    }
}
