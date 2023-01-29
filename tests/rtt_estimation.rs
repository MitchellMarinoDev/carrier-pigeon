use crate::helper::create_client_server_pair;
use simple_logger::SimpleLogger;
use std::process::Command;
use std::time::{Duration, Instant};

mod helper;

#[test]
#[cfg(target_os = "linux")]
fn test_rtt_calculation() {
    // Create a simple logger
    SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init()
        .unwrap();

    let (mut client, mut server) = create_client_server_pair();

    // simulate a 35ms delay
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc add dev lo root netem delay 5ms")
        .output()
        .expect("failed to run `tc` to emulate an unstable network on the `lo` adapter");

    let start = Instant::now();
    // run for a second.
    let time = Duration::from_millis(1_000);
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
