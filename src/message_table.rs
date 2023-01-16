use crate::message_table::MsgRegError::TypeAlreadyRegistered;
use crate::net::{DeserFn, SerFn};
use crate::MId;
use hashbrown::HashMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{type_name, Any, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{Error, ErrorKind};
use MsgRegError::NonUniqueIdentifier;

/// Delivery guarantees.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
pub enum Guarantees {
    /// Messages are guaranteed to arrive. Not necessarily in order.
    Reliable,
    /// Messages are guaranteed to arrive in order.
    ReliableOrdered,
    /// The most recent (newest) message is guaranteed to arrive.
    /// You might not get all the messages if they arrive out of order.
    ReliableNewest,
    /// No guarantees about delivery. Messages might or might not arrive.
    Unreliable,
    /// No delivery guarantee, but you will only get the newest message.
    /// If an older message arrives after a newer one, it will be discarded.
    UnreliableNewest,
}

impl Guarantees {
    /// Weather this guarantee is reliable.
    pub fn reliable(&self) -> bool {
        use Guarantees::*;
        match self {
            Reliable | ReliableOrdered | ReliableNewest => true,
            Unreliable | UnreliableNewest => false,
        }
    }

    /// Weather this guarantee is unreliable.
    pub fn unreliable(&self) -> bool {
        !self.reliable()
    }

    /// Weather this guarantee is ordered.
    pub fn ordered(&self) -> bool {
        use Guarantees::*;
        match self {
            ReliableOrdered => true,
            Reliable | ReliableNewest | Unreliable | UnreliableNewest => false,
        }
    }

    /// Weather this guarantee is newest.
    pub fn newest(&self) -> bool {
        use Guarantees::*;
        match self {
            ReliableNewest | UnreliableNewest => true,
            Reliable | ReliableOrdered | Unreliable => false,
        }
    }

    /// Weather this guarantee is not ordered or newest.
    pub fn no_ordering(&self) -> bool {
        use Guarantees::*;
        match self {
            Reliable | Unreliable => true,
            ReliableOrdered | ReliableNewest | UnreliableNewest => false,
        }
    }
}

/// A registration in the [`MsgTableBuilder`].
#[derive(Copy, Clone, Debug, Hash)]
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

/// The table mapping [`TypeId`]s to message ids ([`MId`]s) and [`Guarantees`].
///
/// You can build this by registering your types with a [`MsgTableBuilder`] or [`SortedMsgTableBuilder`], then building it with
/// [`MsgTableBuilder::build()`] or [`SortedMsgTableBuilder::build()`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct MsgTable {
    /// The mapping from TypeId to MessageId.
    pub tid_map: HashMap<TypeId, MId>,
    /// The transport associated with each message type.
    pub guarantees: Vec<Guarantees>,
    /// The serialization functions associated with each message type.
    pub ser: Vec<SerFn>,
    /// The deserialization functions associated with each message type.
    pub deser: Vec<DeserFn>,
}

pub const CONNECTION_TYPE_MID: MId = 0;
pub const RESPONSE_TYPE_MID: MId = 1;
pub const DISCONNECT_TYPE_MID: MId = 2;

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
        T: Any + Send + Sync + DeserializeOwned + Serialize,
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
    pub fn register_ordered<T>(&mut self, guarantees: Guarantees) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
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
        identifier: String,
        guarantees: Guarantees,
    ) -> Result<(), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        self.sorted
            .push(self.get_sorted_registration::<T>(identifier, guarantees)?);
        Ok(())
    }

    /// Builds the things needed for an ordered registration.
    fn get_ordered_registration<T>(
        &self,
        guarantees: Guarantees,
    ) -> Result<Registration, MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Check if it has been registered already.
        let tid = TypeId::of::<T>();
        if self.tid_registered(tid) {
            return Err(TypeAlreadyRegistered(tid));
        }

        // Get the serialize and deserialize functions
        let deser: DeserFn = |bytes: &[u8]| {
            bincode::deserialize::<T>(bytes)
                .map(|d| Box::new(d) as Box<dyn Any + Send + Sync>)
                .map_err(|o| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("Deser Error: {}", o))
                })
        };
        let ser: SerFn = |m: &(dyn Any + Send + Sync), buf| {
            bincode::serialize_into(
                buf,
                m.downcast_ref::<T>()
                    .expect("wrong type passed to the message serialization function"),
            )
            .map_err(|o| io::Error::new(io::ErrorKind::InvalidData, format!("Ser Error: {}", o)))
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
        identifier: String,
        guarantees: Guarantees,
    ) -> Result<(String, Registration), MsgRegError>
    where
        T: Any + Send + Sync + DeserializeOwned + Serialize,
    {
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
    ///  - `R` is the response message type.
    ///  - `D` is the disconnect message type.
    ///
    /// The generic parameters should **not** be registered before hand.
    ///
    /// This fails iff the generic parameters have already been registered.
    pub fn build<C, R, D>(mut self) -> Result<MsgTable, MsgRegError>
    where
        C: Any + Send + Sync + DeserializeOwned + Serialize,
        R: Any + Send + Sync + DeserializeOwned + Serialize,
        D: Any + Send + Sync + DeserializeOwned + Serialize,
    {
        // Always prepend the Connection and Disconnect types first.
        // This gives them universal MIds.
        let con_discon_types = [
            self.get_ordered_registration::<C>(Guarantees::Reliable)?,
            self.get_ordered_registration::<R>(Guarantees::Reliable)?,
            self.get_ordered_registration::<D>(Guarantees::Reliable)?,
        ];

        let registration_count = self.ordered.len() + self.sorted.len() + 3;
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
            tid_map,
            guarantees,
            ser,
            deser,
        })
    }
}

impl MsgTable {
    /// Gets the number of registered `MId`s.
    #[inline]
    pub fn mid_count(&self) -> usize {
        self.guarantees.len()
    }

    /// Checks if the [`MId`] `mid` is valid.
    #[inline]
    pub fn valid_mid(&self, mid: MId) -> bool {
        mid <= self.mid_count()
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
    pub fn check_type<T>(&self) -> io::Result<()> {
        let tid = TypeId::of::<T>();
        if !self.valid_tid(tid) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Type ({}) not registered as a message.", type_name::<T>()),
            ));
        }
        Ok(())
    }

    pub fn check_mid(&self, mid: MId) -> io::Result<()> {
        if !self.valid_mid(mid) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Message specified MId {}, but the maximum MId is {}.",
                    mid,
                    self.mid_count()
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
