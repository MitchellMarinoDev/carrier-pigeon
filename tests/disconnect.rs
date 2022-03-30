//! Disconnect and Drop tests.
use simple_logger::SimpleLogger;
use std::time::Duration;
use crate::helper::create_client_server_pair;
use crate::helper::test_packets::Disconnect;

mod helper;

#[test]
fn graceful_disconnect() {
    // Create a simple logger
    let _ = SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init();

    {
        // Client Disconnect Test
        let (mut client, mut server) = create_client_server_pair();

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
        let (mut client, mut server) = create_client_server_pair();

        server
            .disconnect(&Disconnect::new("Testing Disconnect Server."), 1)
            .unwrap();

        // Give the server enough time to send the disconnect packet.
        std::thread::sleep(Duration::from_millis(100));

        assert_eq!(
            client.status().disconnected().unwrap(),
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

    {
        // Server Drop Client.
        let (client, server) = create_client_server_pair();
        drop(server);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        // Make sure the client is dropped abruptly
        assert!(client.status().dropped());
    }

    {
        // Client Drop Server.
        let (client, mut server) = create_client_server_pair();
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
