//! Disconnect and Drop tests.
use crate::helper::create_client_server_pair;
use crate::helper::test_messages::Disconnect;
use log::debug;
use std::time::Duration;

mod helper;

#[test]
fn graceful_disconnect() {
    env_logger::init();

    {
        // Client Disconnect Test
        let (mut client, mut server) = create_client_server_pair();
        debug!("Client server pair created.");

        client
            .disconnect(&Disconnect::new("Testing Disconnect Client."))
            .unwrap();

        // Give the client enough time to send the disconnect message.
        std::thread::sleep(Duration::from_millis(100));

        server.tick();
        let discon_count = server.handle_disconnects(|_cid, status| {
            assert_eq!(
                status.disconnected(),
                Some(&Disconnect::new("Testing Disconnect Client."))
            );
        });

        assert_eq!(discon_count, 1);
    }

    {
        // Server Disconnect Test
        let (mut client, mut server) = create_client_server_pair();

        server
            .disconnect(&Disconnect::new("Testing Disconnect Server."), 1)
            .unwrap();

        // Give the server enough time to send the disconnect message.
        std::thread::sleep(Duration::from_millis(100));

        client.tick();
        assert_eq!(
            client.get_status().disconnected::<Disconnect>().unwrap(),
            &Disconnect::new("Testing Disconnect Server.")
        );
    }
}

#[test]
fn drop_test() {
    env_logger::init();

    {
        // Server Drop Client.
        let (mut client, server) = create_client_server_pair();
        drop(server);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        client.tick();
        // Make sure the client is dropped abruptly
        assert!(client.get_status().dropped().is_some());
    }

    {
        // Client Drop Server.
        let (client, mut server) = create_client_server_pair();
        drop(client);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        server.tick();
        let counts = server.handle_disconnects(|_cid, status| {
            assert!(status.dropped().is_some(), "Expected status to be dropped");
        });

        // make sure there was 1 disconnect handled.
        assert_eq!(counts, 1);
    }
}
