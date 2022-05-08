//! An attempt to serialize the current time as a u16.
//!
//! To do this, we send the least significant 16 bits of the unix millis. Then on the receiving
//! side, we can assume that the next 16 bits of the send time are going to be the same, or 1 less.

use std::time::{SystemTime, UNIX_EPOCH};

/// Gets the current unix millis as a u32.
pub fn unix_millis() -> u32 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current system time is earlier than the UNIX_EPOCH")
        .as_millis();
    (millis & 0xFFFF_FFFF) as u32
}

/// Uses the 16 least significant bits of the unix millis time stamp, and reconstructs rest.
pub(crate) fn reconstruct_millis(lsb: u16) -> u32 {
    reconstruct_millis_inner(unix_millis(), lsb)
}

/// The logic for reconstructing the millis.
///
/// Separated to allow testing.
#[inline]
fn reconstruct_millis_inner(now: u32, lsb: u16) -> u32 {
    let now_lsb = (now & 0xFFFF) as u16;
    let mut msb = now & 0xFFFF_0000;
    if lsb > now_lsb {
        msb -= 0x0001_0000;
    }
    msb | lsb as u32
}

#[cfg(test)]
mod tests {
    use crate::time::reconstruct_millis_inner;

    #[test]
    fn test_reconstruction() {
        let points: Vec<(u32, u32)> = vec![
            // Send stamp, Recv stamp
            (0x89F9_8103, 0x89F9_8103),
            (0x3F11_7904, 0x3F11_9035),
            (0x23AC_9723, 0x23AD_1039),
            (0x1839_8183, 0x183A_4039),
        ];

        for point in points {
            // Test reconstruction
            let lsb = (point.0 & 0xFFFF) as u16;
            let reconstructed = reconstruct_millis_inner(point.1, lsb);
            assert_eq!(
                reconstructed, point.0,
                "Send:\t\t\t {:#10x},\nReceive:\t\t {:#10x},\nReconstructed:\t {:#10x}",
                point.0, point.1, reconstructed
            );
        }
    }
}
