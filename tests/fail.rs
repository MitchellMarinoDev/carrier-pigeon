use std::io::ErrorKind;
use carrier_pigeon::Client;
use carrier_pigeon::net::CConfig;
use crate::helper::test_messages::{Connection, get_table_parts, Response};

mod helper;

#[test]
fn client_fail() {
    let parts = get_table_parts();

    let client = Client::new("127.0.0.1:7778".parse().unwrap(), parts, CConfig::default(), Connection::new("John Smith"));
    let result = client.block::<Response>();
    println!("Error: {:?}", result);
    assert_eq!(result.unwrap_err().kind(), ErrorKind::ConnectionRefused);
}