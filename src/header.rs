use crate::MId;

/// The number of bytes the header takes up.
pub const HEADER_LEN: usize = 4;

/// A header to be sent before the actual contents of the packet.
///
/// `len` and `mid` are sent as big endian u16s.
/// This means they have a max value of **`65535`**.
/// This shouldn't pose any real issues.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct Header {
    /// The message id.
    pub mid: MId,
    /// Then length of the packet ***without the header***.
    pub len: usize,
}

impl Header {
    /// Creates a [`Header`] with the given [`MId`] and `length`.
    pub fn new(mid: MId, len: usize) -> Self {
        Header { mid, len }
    }

    /// Converts the [`Header`] to big endian bytes to be sent over
    /// the internet.
    pub fn to_be_bytes(&self) -> [u8; HEADER_LEN] {
        let mid_b = (self.mid as u16).to_be_bytes();
        let len_b = (self.len as u16).to_be_bytes();

        [mid_b[0], mid_b[1], len_b[0], len_b[1]]
    }

    /// Converts the big endian bytes back into a [`Header`].
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), HEADER_LEN);

        let mid = u16::from_be_bytes(bytes[..2].try_into().unwrap()) as usize;
        let len = u16::from_be_bytes(bytes[2..].try_into().unwrap()) as usize;

        Header { mid, len }
    }
}

#[cfg(test)]
mod tests {
    use crate::header::Header;

    #[test]
    fn to_from_bytes() {
        let points = vec![(0, 0), (2, 2), (100, 34), (65530, 982)];

        for point in points {
            let header = Header::new(point.0, point.1);
            let ser = header.to_be_bytes();
            let de = Header::from_be_bytes(&ser);
            assert_eq!(header, de);
        }
    }
}
