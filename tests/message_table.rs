//! Tests for testing the functionality of the [`MsgTable`] and [`SortedMsgTable`].
use crate::helper::test_messages::{Connection, Disconnect, ReliableMsg, Response, UnreliableMsg};
use carrier_pigeon::MsgRegError::{NonUniqueIdentifier, TypeAlreadyRegistered};
use carrier_pigeon::Transport::{TCP, UDP};
use carrier_pigeon::{Guarantees, MsgRegError, MsgTable, MsgTableBuilder};
use hashbrown::HashMap;
use std::any::TypeId;

mod helper;

/// Tests the [`TypeAlreadyRegistered`], and [`NonUniqueIdentifier`]
/// errors for both MsgTable variants.
#[test]
fn errors() {
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    assert_eq!(
        builder.register_ordered::<ReliableMsg>(Guarantees::Reliable),
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
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    table
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<ReliableMsg>(), 3);
    tid_map.insert(TypeId::of::<UnreliableMsg>(), 4);

    let guarantees = vec![
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Unreliable,
    ];

    let parts = table.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.guarantees, guarantees);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
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
    expected_tid_map.insert(TypeId::of::<Response>(), 1);
    expected_tid_map.insert(TypeId::of::<Disconnect>(), 2);
    expected_tid_map.insert(TypeId::of::<ReliableMsg>(), 3);
    expected_tid_map.insert(TypeId::of::<UnreliableMsg>(), 4);

    let expected_guarantees = vec![
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Unreliable,
    ];

    let table1 = builder1
        .build::<Connection, Response, Disconnect>()
        .unwrap();
    let table2 = builder2
        .build::<Connection, Response, Disconnect>()
        .unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(table1.tid_map, expected_tid_map);
    assert_eq!(table1.guarantees, expected_guarantees);
    // Can't check ser and deser functions.
    assert_eq!(table1.ser.len(), 5);
    assert_eq!(table1.deser.len(), 5);

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
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    let mut builder2 = MsgTableBuilder::new();
    builder2
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();

    builder1.join(&builder2).unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<ReliableMsg>(), 3);
    tid_map.insert(TypeId::of::<UnreliableMsg>(), 4);

    let expected_guarantees = vec![
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Unreliable,
    ];

    let parts = builder1
        .build::<Connection, Response, Disconnect>()
        .unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.guarantees, expected_guarantees);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
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
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<ReliableMsg>(), 3);
    tid_map.insert(TypeId::of::<UnreliableMsg>(), 4);

    let expected_guarantees = vec![
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Reliable,
        Guarantees::Unreliable,
    ];

    let table = builder1
        .build::<Connection, Response, Disconnect>()
        .unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(table.tid_map, tid_map);
    assert_eq!(table.guarantees, expected_guarantees);
    // Can't check ser and deser functions.
    assert_eq!(table.ser.len(), 5);
    assert_eq!(table.deser.len(), 5);
}

/// Tests [`MsgTable`] joining.
#[test]
fn join_error() {
    // ordered
    let mut builder1 = MsgTableBuilder::new();
    builder1
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    let mut builder2 = MsgTableBuilder::new();
    builder2
        .register_ordered::<ReliableMsg>(Guarantees::Unreliable)
        .unwrap();

    assert_eq!(builder1.join(&builder2), Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>())));

    // sorted
    let mut builder1 = MsgTableBuilder::new();
    builder1
        .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
        .unwrap();
    let mut builder2 = MsgTableBuilder::new();
    builder2
        .register_sorted::<ReliableMsg>(Guarantees::Unreliable, "tests::DifferentMsg")
        .unwrap();

    assert_eq!(builder1.join(&builder2), Err(TypeAlreadyRegistered(TypeId::of::<ReliableMsg>())));

    // sorted
    let mut builder1 = MsgTableBuilder::new();
    builder1
        .register_sorted::<ReliableMsg>(Guarantees::Reliable, "tests::ReliableMsg")
        .unwrap();
    let mut builder2 = MsgTableBuilder::new();
    builder2
        .register_sorted::<UnreliableMsg>(Guarantees::Unreliable, "tests::ReliableMsg")
        .unwrap();

    assert_eq!(builder1.join(&builder2).unwrap_err(), NonUniqueIdentifier("tests::ReliableMsg".to_owned()));
}
