//! Disconnect and Drop tests.
use crate::helper::create_client_server_pair;
use crate::helper::test_messages::Disconnect;
use carrier_pigeon::NetConfig;
use std::time::Duration;

mod helper;

#[test]
fn graceful_disconnect() {
    let _ = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init();

    let config = NetConfig {
        ack_send_count: 2,
        pings_to_retain: 8,
        ping_smoothing_value: 4,
        ping_interval: Duration::from_millis(1),
        recv_timeout: Duration::from_millis(10),
        ack_msg_interval: Duration::from_millis(1),
    };

    {
        // Client Disconnect Test
        let (mut client, mut server) = create_client_server_pair(config);

        client
            .disconnect(&Disconnect::new("Testing Disconnect Client."))
            .unwrap();

        // Give the client enough time to send the disconnect message.
        std::thread::sleep(Duration::from_millis(100));

        server.tick();
        let mut discon_count = 0;
        while let Some(discon_event) = server.handle_disconnect() {
            assert_eq!(
                discon_event.disconnection_type.unwrap_disconnected(),
                Some(&Disconnect::new("Testing Disconnect Client."))
            );
            discon_count += 1;
        }
        assert_eq!(discon_count, 1);
    }

    {
        // Server Disconnect Test
        let (mut client, mut server) = create_client_server_pair(config);

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
    let _ = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init();

    let config = NetConfig {
        ack_send_count: 2,
        pings_to_retain: 8,
        ping_smoothing_value: 4,
        ping_interval: Duration::from_millis(1),
        recv_timeout: Duration::from_millis(10),
        ack_msg_interval: Duration::from_millis(1),
    };

    {
        // Server Drop Client.
        let (mut client, server) = create_client_server_pair(config);
        drop(server);

        // Give the client a few ticks to timeout or detect the connection drop.
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(10));
            client.tick();
            // exit early if the drop was detected.
            if client.get_status().is_dropped() {
                break;
            }
        }
        // Make sure the client is dropped abruptly
        assert!(client.get_status().is_dropped());
    }

    {
        // Client Drop Server.
        let (client, mut server) = create_client_server_pair(config);
        drop(client);

        // Give the server a few ticks to timeout or detect the connection drop.
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(10));
            server.tick();
        }

        let mut discon_count = 0;
        while let Some(discon_event) = server.handle_disconnect() {
            assert!(
                discon_event.disconnection_type.is_dropped(),
                "expected status to be dropped"
            );
            discon_count += 1;
        }

        // make sure there was 1 disconnect handled.
        assert_eq!(discon_count, 1);
    }
}
