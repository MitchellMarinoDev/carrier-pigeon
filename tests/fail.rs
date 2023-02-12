use crate::helper::test_messages::{get_msg_table, Connection};
use carrier_pigeon::net::ClientConfig;
use carrier_pigeon::Client;
use std::thread::sleep;
use std::time::Duration;

mod helper;

#[test]
fn client_fail() {
    let table = get_msg_table();

    let local = "127.0.0.1:7776".parse().unwrap();
    let peer = "127.0.0.1:7777".parse().unwrap();

    let mut client = Client::new(ClientConfig::default(), table);
    client
        .connect(local, peer, &Connection::new("John Smith"))
        .unwrap();

    // Block until the connection is made.
    let mut status = client.get_status();
    while status.is_connecting() {
        sleep(Duration::from_millis(1));
        status = client.get_status();
    }

    let status = client.handle_status();
    assert!(status.is_connection_failed());
}
