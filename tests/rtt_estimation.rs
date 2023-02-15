use crate::helper::create_client_server_pair;
use carrier_pigeon::NetConfig;
use std::process::Command;
use std::time::{Duration, Instant};

mod helper;

#[test]
#[cfg(target_os = "linux")]
fn test_rtt_calculation() {
    let _ = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init();

    let config = NetConfig {
        ack_send_count: 2,
        pings_to_retain: 8,
        ping_smoothing_value: 4,
        ping_interval: Duration::from_millis(1),
        recv_timeout: Duration::from_millis(20),
        ack_msg_interval: Duration::from_millis(1),
    };

    let (mut client, mut server) = create_client_server_pair(config);

    // simulate a 35ms delay
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc add dev lo root netem delay 5ms")
        .output()
        .expect("failed to run `tc` to emulate an unstable network on the `lo` adapter");

    let start = Instant::now();
    // run for a second.
    let time = Duration::from_millis(200);
    loop {
        if start.elapsed() > time {
            break;
        }
        server.tick();
        client.tick();
    }

    // remove the simulated conditions
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc del dev lo root netem")
        .output()
        .expect("failed to run `tc` to remove the emulated network conditions on the `lo` adapter");

    server
        .cid_addr_pairs()
        .for_each(|(cid, addr)| println!("{}: {}", cid, addr));
    let server_rtt = server.rtt(1).unwrap() as i32;
    let client_rtt = client.rtt() as i32;
    // Double the 5ms would be 10_000 us.
    assert!(
        server_rtt - 10_000 < 100,
        "Server rtt was {} (expected 10_000 +/- 100)",
        server_rtt
    );
    assert!(
        client_rtt - 10_000 < 100,
        "Client rtt was {} (expected 10_000 +/- 100)",
        client_rtt
    );
}
