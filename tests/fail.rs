use std::io::ErrorKind;
use carrier_pigeon::Client;
use crate::helper::ADDR_LOCAL;
use crate::helper::test_packets::{Connection, get_table_parts};

mod helper;

#[test]
fn client_fail() {
    let parts = get_table_parts();

    let client = Client::new(ADDR_LOCAL.parse().unwrap(), parts, Connection::new("John Smith"));
    let result = client.block();
    println!("Error: {:?}", result);
    assert_eq!(result.unwrap_err().kind(), ErrorKind::ConnectionRefused);
}