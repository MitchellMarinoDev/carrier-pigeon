use crate::test_packets::{get_table_parts, Connection, Disconnect, Response};
use simple_logger::SimpleLogger;
use std::time::Duration;
use tokio::runtime::Handle;

mod test_packets;

///! Disconnect and Drop tests.

const ADDR_LOCAL: &str = "127.0.0.1:0";

type Client = carrier_pigeon::Client<Connection, Response, Disconnect>;
type Server = carrier_pigeon::Server<Connection, Response, Disconnect>;

#[test]
fn graceful_disconnect() {
    // Create a simple logger
    let _ = SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    {
        // Client Disconnect Test
        let (mut client, mut server) = create_client_server_pair(rt.clone());

        client
            .disconnect(&Disconnect::new("Testing Disconnect Client."))
            .unwrap();

        // Give the client enough time to send the disconnect packet.
        std::thread::sleep(Duration::from_millis(100));

        let counts = server.handle_disconnects(
            &mut |_cid, discon_msg| {
                assert_eq!(*discon_msg, Disconnect::new("Testing Disconnect Client."));
            },
            &mut |_cid, _error| {
                panic!("No connections were supposed to be dropped");
            },
        );

        assert_eq!(counts, (1, 0));
    }

    {
        // Server Disconnect Test
        let (mut client, mut server) = create_client_server_pair(rt.clone());

        server
            .disconnect(&Disconnect::new("Testing Disconnect Server."), 1)
            .unwrap();

        // Give the server enough time to send the disconnect packet.
        std::thread::sleep(Duration::from_millis(100));

        assert_eq!(
            client.get_disconnect().unwrap().as_ref().unwrap(),
            &Disconnect::new("Testing Disconnect Server.")
        );
    }
}

#[test]
fn drop_test() {
    // Create a simple logger
    let _ = SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    {
        // Server Drop Client.
        let (mut client, server) = create_client_server_pair(rt.clone());
        drop(server);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        assert!(client.get_disconnect().unwrap().is_err());
    }

    {
        // Client Drop Server.
        let (client, mut server) = create_client_server_pair(rt.clone());
        drop(client);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        let counts = server.handle_disconnects(
            &mut |_cid, _msg| {
                panic!("There should not be a gracefull disconnect.");
            },
            &mut |_cid, _e| {
                println!("Dropped connection with error");
            },
        );

        // make sure there was 1 drop and 0 graceful disconnects.
        assert_eq!(counts, (0, 1));
    }
}

/// Creates a client and server that are connected to each other.
/// Panics if any issues occur.
fn create_client_server_pair(rt: Handle) -> (Client, Server) {
    let parts = get_table_parts();

    // Create server.
    let mut server = Server::new(ADDR_LOCAL.parse().unwrap(), parts.clone(), rt.clone())
        .blocking_recv()
        .unwrap()
        .unwrap();
    let addr = server.listen_addr();
    println!("Server created on addr: {}", addr);

    // Start client connection.
    let client = Client::new(addr, parts, Connection::new("John"), rt);

    // Spin until the connection is handled.
    // Normally this would be done in the game loop
    // and there would be other things to do.
    while 0 == server.handle_new_cons(&mut |_con_msg| (true, Response::Accepted)) {}

    // Finish the client connection.
    let (client, response_msg) = client.blocking_recv().unwrap().unwrap();

    assert_eq!(response_msg, Response::Accepted);

    (client, server)
}
