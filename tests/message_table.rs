//! Tests for testing the functionality of the [`MsgTable`] and [`SortedMsgTable`].
use std::any::TypeId;
use hashbrown::HashMap;
use carrier_pigeon::{MsgRegError, MsgTable, SortedMsgTable};
use carrier_pigeon::Transport::{TCP, UDP};
use crate::helper::test_packets::{Connection, Disconnect, Response, TcpPacket, UdpPacket};

mod helper;

/// Tests the [`TypeAlreadyRegistered`], and [`NonUniqueIdentifier`]
/// errors for both MsgTable variants.
#[test]
fn errors() {
    let mut table = MsgTable::new();
    table.register::<TcpPacket>(TCP).unwrap();
    assert_eq!(table.register::<TcpPacket>(TCP), Err(MsgRegError::TypeAlreadyRegistered));

    let mut table = SortedMsgTable::new();
    table.register::<TcpPacket>(TCP, "test::TcpPacket").unwrap();
    assert_eq!(table.register::<TcpPacket>(TCP, "test::TcpPacket2"), Err(MsgRegError::TypeAlreadyRegistered));

    let mut table = SortedMsgTable::new();
    table.register::<TcpPacket>(TCP, "test::TcpPacket").unwrap();
    assert_eq!(table.register::<UdpPacket>(TCP, "test::TcpPacket"), Err(MsgRegError::NonUniqueIdentifier));
}

/// Tests MsgTableParts generation.
#[test]
fn parts_gen() {
    let mut table = MsgTable::new();
    table.register::<TcpPacket>(TCP).unwrap();
    table.register::<UdpPacket>(UDP).unwrap();


    // Expected result:
    let mut tid_map = HashMap::new();
    tid_map.insert(TypeId::of::<Connection>(), 0);
    tid_map.insert(TypeId::of::<Response>(), 1);
    tid_map.insert(TypeId::of::<Disconnect>(), 2);
    tid_map.insert(TypeId::of::<TcpPacket>(), 3);
    tid_map.insert(TypeId::of::<UdpPacket>(), 4);

    let transports = vec![
        TCP,
        TCP,
        TCP,
        TCP,
        UDP,
    ];

    let parts = table.build::<Connection, Response, Disconnect>().unwrap();

    // Make sure the tid_map and transports generated correctly.
    assert_eq!(parts.tid_map, tid_map);
    assert_eq!(parts.transports, transports);
    // Can't check ser and deser functions.
    assert_eq!(parts.ser.len(), 5);
    assert_eq!(parts.deser.len(), 5);
}
