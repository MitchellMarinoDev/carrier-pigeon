//! Disconnect and Drop tests.
use crate::helper::create_client_server_pair;
use crate::helper::test_messages::Disconnect;
use std::time::Duration;

mod helper;

#[test]
fn graceful_disconnect() {
    std::env::set_var("RUST_LOG", "TRACE");
    env_logger::init();

    {
        // Client Disconnect Test
        let (mut client, mut server) = create_client_server_pair();

        client
            .disconnect(&Disconnect::new("Testing Disconnect Client."))
            .unwrap();

        // Give the client enough time to send the disconnect message.
        std::thread::sleep(Duration::from_millis(100));

        server.tick();
        while let Some(disconnect_event) = server.handle_disconnect() {
            assert_eq!(
                disconnect_event.disconnection_type.unwrap_disconnected(),
                Some(&Disconnect::new("Testing Disconnect Client."))
            );
        }

        let mut discon_count = 0;
        while let Some(_) = server.handle_disconnect() {
            discon_count += 1;
        }
        assert_eq!(discon_count, 1);
    }

    {
        // Server Disconnect Test
        let (mut client, mut server) = create_client_server_pair();

        server
            .disconnect(Disconnect::new("Testing Disconnect Server."), 1)
            .unwrap();

        // Give the server enough time to send the disconnect message.
        std::thread::sleep(Duration::from_millis(100));

        client.tick();
        assert_eq!(
            client.get_status().unwrap_disconnected(),
            Some(&Disconnect::new("Testing Disconnect Server."))
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
        assert!(client.get_status().is_dropped());
    }

    {
        // Client Drop Server.
        let (client, mut server) = create_client_server_pair();
        drop(client);

        // Give the server enough time for the connection to sever.
        std::thread::sleep(Duration::from_millis(100));

        server.tick();
        while let Some(discon_msg) = server.handle_disconnect() {
            assert!(discon_msg.disconnection_type.is_dropped())
        }

        let mut discon_count = 0;
        while let Some(discon_event) = server.handle_disconnect() {
            assert!(discon_event.disconnection_type.is_dropped(), "expected status to be dropped");
            discon_count += 1;
        }

        // make sure there was 1 disconnect handled.
        assert_eq!(discon_count, 1);
    }
}
