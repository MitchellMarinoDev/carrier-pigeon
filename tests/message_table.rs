//! Tests for testing the functionality of the [`MsgTable`] and [`SortedMsgTable`].
use crate::helper::test_messages::{Connection, Disconnect, Response, TcpMsg, UdpMsg};
use carrier_pigeon::MsgRegError::{NonUniqueIdentifier, TypeAlreadyRegistered};
use carrier_pigeon::Transport::{TCP, UDP};
use carrier_pigeon::{MsgRegError, MsgTable, SortedMsgTable};
use hashbrown::HashMap;
use std::any::TypeId;

mod helper;

/// Tests the [`TypeAlreadyRegistered`], and [`NonUniqueIdentifier`]
/// errors for both MsgTable variants.
#[test]
fn errors() {
    let mut table = MsgTable::new();
    table.register::<TcpMsg>(TCP).unwrap();
    assert_eq!(
        table.register::<TcpMsg>(TCP),
        Err(MsgRegError::TypeAlreadyRegistered)
    );

    let mut table = SortedMsgTable::new();
    table.register::<TcpMsg>(TCP, "test::TcpMsg").unwrap();
    assert_eq!(
        table.register::<TcpMsg>(TCP, "test::TcpMsg2"),
        Err(MsgRegError::TypeAlreadyRegistered)
    );

    let mut table = SortedMsgTable::new();
    table.register::<TcpMsg>(TCP, "test::TcpMsg").unwrap();
    assert_eq!(
        table.register::<UdpMsg>(TCP, "test::TcpMsg"),
        Err(MsgRegError::NonUniqueIdentifier)
    );
}

/// Tests [`MsgTableParts`] generation.
#[test]
fn parts_gen() {
    let mut table = MsgTable::new();
    table.register::<TcpMsg>(TCP).unwrap();
    table.register::<UdpMsg>(UDP).unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<TcpMsg>(), 3);
    tid_map.insert(TypeId::of::<UdpMsg>(), 4);

    let transports = vec![TCP, TCP, TCP, TCP, UDP];

    let parts = table.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.transports, transports);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
}

/// Tests [`MsgTableParts`] generation.
#[test]
fn parts_gen_sorted() {
    let mut table1 = SortedMsgTable::new();
    table1.register::<TcpMsg>(TCP, "tests::TcpMsg").unwrap();
    table1.register::<UdpMsg>(UDP, "tests::UdpMsg").unwrap();

    // Different order.
    let mut table2 = SortedMsgTable::new();
    table2.register::<UdpMsg>(UDP, "tests::UdpMsg").unwrap();
    table2.register::<TcpMsg>(TCP, "tests::TcpMsg").unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<TcpMsg>(), 3);
    tid_map.insert(TypeId::of::<UdpMsg>(), 4);

    let transports = vec![TCP, TCP, TCP, TCP, UDP];

    let parts = table1.build::<Connection, Response, Disconnect>().unwrap();
    let parts2 = table2.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.transports, transports);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);

    // Make sure the tables generated the same.
    assert_eq!(parts.tid_map, parts2.tid_map);
    assert_eq!(parts.transports, parts2.transports);
}

/// Tests [`MsgTable`] joining.
#[test]
fn join() {
    // MsgTable
    let mut table1 = MsgTable::new();
    table1.register::<TcpMsg>(TCP).unwrap();
    let mut table2 = MsgTable::new();
    table2.register::<UdpMsg>(UDP).unwrap();

    table1.join(&table2).unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<TcpMsg>(), 3);
    tid_map.insert(TypeId::of::<UdpMsg>(), 4);

    let transports = vec![TCP, TCP, TCP, TCP, UDP];

    let parts = table1.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.transports, transports);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
}

/// Tests [`SortedMsgTable`] joining.
#[test]
fn join_sorted() {
    // MsgTable
    let mut table1 = SortedMsgTable::new();
    table1.register::<TcpMsg>(TCP, "tests::TcpMsg").unwrap();
    let mut table2 = SortedMsgTable::new();
    table2.register::<UdpMsg>(UDP, "tests::UdpMsg").unwrap();

    table1.join(&table2).unwrap();

    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<TcpMsg>(), 3);
    tid_map.insert(TypeId::of::<UdpMsg>(), 4);

    let transports = vec![TCP, TCP, TCP, TCP, UDP];

    let parts = table1.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.transports, transports);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
}

/// Tests [`MsgTable`] joining.
#[test]
fn join_error() {
    // MsgTable
    let mut table1 = MsgTable::new();
    table1.register::<TcpMsg>(TCP).unwrap();
    let mut table2 = MsgTable::new();
    table2.register::<TcpMsg>(UDP).unwrap();

    assert_eq!(table1.join(&table2).unwrap_err(), TypeAlreadyRegistered);

    // SortedMsgTable
    let mut table1 = SortedMsgTable::new();
    table1.register::<TcpMsg>(TCP, "tests::TcpMsg").unwrap();
    let mut table2 = SortedMsgTable::new();
    table2
        .register::<TcpMsg>(UDP, "tests::OtherTcpMsg")
        .unwrap();

    assert_eq!(table1.join(&table2).unwrap_err(), TypeAlreadyRegistered);

    // SortedMsgTable
    let mut table1 = SortedMsgTable::new();
    table1.register::<TcpMsg>(TCP, "tests::TcpMsg").unwrap();
    let mut table2 = SortedMsgTable::new();
    table2.register::<UdpMsg>(UDP, "tests::TcpMsg").unwrap();

    assert_eq!(table1.join(&table2).unwrap_err(), NonUniqueIdentifier);
}
