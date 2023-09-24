use crate::helper::create_client_server_pair;
use carrier_pigeon::NetConfig;
use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::time::Duration;

use crate::helper::test_messages::UnreliableMsg;

mod helper;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("single_udp_big", single_udp_big);
    c.bench_function("single_udp_small", single_udp_small);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

const CONFIG: NetConfig = NetConfig {
    ack_send_count: 2,
    pings_to_retain: 8,
    ping_smoothing_value: 4,
    ping_interval: Duration::from_millis(1),
    recv_timeout: Duration::from_millis(20),
    ack_msg_interval: Duration::from_millis(1),
};

fn single_udp_big(b: &mut Bencher) {
    let (mut client, mut server) = create_client_server_pair(CONFIG);

    let string: String = vec!['A'; 504].into_iter().collect();
    let msg = UnreliableMsg::new(string);

    b.iter(|| {
        client.send(&msg).unwrap();
        client.tick();
        while server.recv::<UnreliableMsg>().count() == 0 {
            server.tick();
            client.tick();
        }
        let msg = server.recv::<UnreliableMsg>().next().unwrap();
        assert_eq!(msg.content, msg.content);
    });
}

fn single_udp_small(b: &mut Bencher) {
    let (mut client, mut server) = create_client_server_pair(CONFIG);

    let string: String = vec!['A'; 10].into_iter().collect();
    let msg = UnreliableMsg::new(string);

    b.iter(|| {
        client.send(&msg).unwrap();
        client.tick();
        while server.recv::<UnreliableMsg>().count() == 0 {
            server.tick();
            client.tick();
        }
        let msg = server.recv::<UnreliableMsg>().next().unwrap();
        assert_eq!(msg.content, msg.content);
    });
}
