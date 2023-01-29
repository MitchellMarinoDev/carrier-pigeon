use std::collections::VecDeque;
use crate::messages::PingMsg;
use crate::CId;
use hashbrown::HashMap;
use log::warn;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PING_INTERVAL: Duration = Duration::from_millis(100);
const PINGS_TO_RETAIN: usize = 8;
/// This is 1/ the number to move by for smooth the ping number. Higher number is smoother.
const PING_LERP_DIST: i32 = 4;

/// Gets the number of microseconds since the unix epoch
fn unix_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current system time is earlier than the UNIX_EPOCH")
        .as_micros()
}

pub(crate) struct ServerPingSystem {
    /// The [`Instant`] of the last ping
    last_ping_time: Instant,
    /// The incrementing integer to use as an identifier for the [`PingMsg`]
    last_ping_counter: u32,
    /// A list of stored sent ping identifiers with the unix micros for when it was sent.
    pings: VecDeque<(u32, u128)>,
    /// The current estimate of the round trip time.
    rtt: HashMap<CId, u32>,
}

impl ServerPingSystem {
    /// Creates a new [`ServerPingSystem`].
    pub(crate) fn new() -> Self {
        ServerPingSystem {
            last_ping_time: Instant::now(),
            last_ping_counter: 0,
            pings: VecDeque::new(),
            rtt: HashMap::new(),
        }
    }

    /// Gets the next [`PingMsg`] to send.
    fn next_ping_msg(&mut self) -> PingMsg {
        self.last_ping_time = Instant::now();
        let ping_num = self.last_ping_counter;
        self.last_ping_counter += 1;
        if self.pings.len() > PINGS_TO_RETAIN {
            self.pings.pop_back();
        }
        self.pings.push_front((ping_num, unix_micros()));

        PingMsg::Req(ping_num)
    }

    /// Gets the next [`PingMsg`] to send if needed.
    pub(crate) fn get_ping_msg(&mut self) -> Option<PingMsg> {
        if self.last_ping_time.elapsed() < PING_INTERVAL {
            return None;
        }
        Some(self.next_ping_msg())
    }

    /// Updates the RTT based on the given [`PingMsg`].
    pub(crate) fn recv_ping_msg(&mut self, cid: CId, msg: PingMsg) {
        let ping_num = match msg {
            PingMsg::Res(ping_num) => ping_num,
            _ => return warn!("`recv_ping_msg` called on a request ping message."),
        };

        if let Some((_, micros)) = self
            .pings
            .iter()
            .filter(|(v_ping_num, _)| *v_ping_num == ping_num)
            .next()
        {
            let elapsed = match unix_micros().checked_sub(*micros) {
                Some(elapsed) => elapsed,
                None => {
                    return warn!("Ping message had a negative RTT. Did the system clock change?")
                }
            };

            self.mod_rtt(cid, elapsed);
        }
    }

    /// Starts tracking the rtt time of `cid`.
    ///
    /// You should call this before calling `recv_ping_msg` with that `cid`.
    pub fn add_cid(&mut self, cid: CId) {
        self.rtt.insert(cid, 0);
    }

    /// Stops tracking the rtt time of `cid`.
    pub fn remove_cid(&mut self, cid: CId) {
        self.rtt.remove(&cid);
    }

    /// Modifies the RTT time to be closer to `micros`. Any smoothing of values should be done here.
    fn mod_rtt(&mut self, cid: CId, micros: u128) {
        let rtt = match self.rtt.get_mut(&cid) {
            None => return warn!("Trying to change the RTT of not connected cid {}", cid),
            Some(rtt) => rtt,
        };

        let dif = micros as i128 - *rtt as i128;
        *rtt = rtt.saturating_add_signed(dif as i32 / PING_LERP_DIST);
    }

    /// Gets the current estimated round trip time in microseconds for the given connection.
    pub fn rtt(&self, cid: CId) -> Option<u32> {
        self.rtt.get(&cid).copied()
    }
}

pub(crate) struct ClientPingSystem {
    /// The [`Instant`] of the last ping
    last_ping_time: Instant,
    /// The incrementing integer to use as an identifier for the [`PingMsg`]
    last_ping_counter: u32,
    /// A list of stored sent ping identifiers with the unix micros for when it was sent.
    pings: Vec<(u32, u128)>,
    /// The current estimate of the round trip time.
    rtt: u32,
}

impl ClientPingSystem {
    /// Creates a new [`ClientPingSystem`].
    pub(crate) fn new() -> Self {
        ClientPingSystem {
            last_ping_time: Instant::now(),
            last_ping_counter: 0,
            pings: vec![],
            rtt: 0,
        }
    }

    /// Gets the next [`PingMsg`] to send.
    fn next_ping_msg(&mut self) -> PingMsg {
        self.last_ping_time = Instant::now();
        let ping_num = self.last_ping_counter;
        self.last_ping_counter += 1;
        self.pings.push((ping_num, unix_micros()));

        PingMsg::Req(ping_num)
    }

    /// Gets the next [`PingMsg`] to send if needed.
    pub(crate) fn get_ping_msg(&mut self) -> Option<PingMsg> {
        if self.last_ping_time.elapsed() < PING_INTERVAL {
            return None;
        }
        Some(self.next_ping_msg())
    }

    /// Updates the RTT based on the given [`PingMsg`].
    pub(crate) fn recv_ping_msg(&mut self, msg: PingMsg) {
        let ping_num = match msg {
            PingMsg::Res(ping_num) => ping_num,
            _ => return warn!("`recv_ping_msg` called on a request ping message."),
        };

        if let Some((_, micros)) = self
            .pings
            .iter()
            .filter(|(v_ping_num, _)| *v_ping_num == ping_num)
            .next()
        {
            let elapsed = match unix_micros().checked_sub(*micros) {
                Some(elapsed) => elapsed,
                None => {
                    return warn!("Ping message had a negative RTT. Did the system clock change?")
                }
            };

            self.mod_rtt(elapsed);
        }
    }

    /// Modifies the RTT time to be closer to `micros`. Any smoothing of values should be done here.
    fn mod_rtt(&mut self, micros: u128) {
        let dif = micros as i128 - self.rtt as i128;
        self.rtt = self.rtt.saturating_add_signed(dif as i32 / PING_LERP_DIST);
    }

    /// Gets the current estimated round trip time in microseconds.
    pub fn rtt(&self) -> u32 {
        self.rtt
    }
}

#[cfg(test)]
mod tests {
    use crate::connection::ping_system::ClientPingSystem;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_rtt() {
        let mut ping_sys = ClientPingSystem::new();
        for _ in 0..20 {
            let ping_msg = ping_sys
                .next_ping_msg()
                .response()
                .expect("PingMsg should be the request variant");
            sleep(Duration::from_micros(20_000));
            ping_sys.recv_ping_msg(ping_msg);
        }
        // RTT should be about 20_000 micros (20ms)
        assert!(ping_sys.rtt() as i32 - 20_000 < 100);
    }
}
