use crate::helper::test_messages::{get_table_parts, Connection, Response};
use carrier_pigeon::net::NetConfig;
use carrier_pigeon::Client;
use std::io::ErrorKind;

mod helper;

#[test]
fn client_fail() {
    let parts = get_table_parts();

    let client = Client::new(
        "127.0.0.1:7778",
        parts,
        NetConfig::default(),
        Connection::new("John Smith"),
    );
    let result = client.block::<Response>();
    println!("Error: {:?}", result);
    assert_eq!(result.unwrap_err().kind(), ErrorKind::ConnectionRefused);
}
