//! One input event as it travels: type, code, value. The kernel stamps the time, so the wire
//! carries the 16 time bytes of `struct input_event` as zeros and the daemon never reads them.

use crate::codes::{EV_SYN, SYN_REPORT};

/// Bytes of one `struct input_event` on a 64-bit kernel.
pub const EVENT_BYTES: usize = 24;
/// Bytes of the timestamp at the front of it.
pub const TIME_BYTES: usize = 16;

/// An event without its timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// `EV_*`.
    pub kind: u16,
    /// The code within the type.
    pub code: u16,
    /// The value.
    pub value: i32,
}

impl Event {
    /// The end-of-frame event.
    pub const SYNC: Event = Event::new(EV_SYN, SYN_REPORT, 0);

    /// An event of `kind` with `code` and `value`.
    pub const fn new(kind: u16, code: u16, value: i32) -> Self {
        Self { kind, code, value }
    }

    /// Whether this is the end-of-frame event.
    pub const fn is_sync(self) -> bool {
        self.kind == EV_SYN && self.code == SYN_REPORT
    }

    /// The 24 bytes of `struct input_event` with a zero time.
    pub fn to_bytes(self) -> [u8; EVENT_BYTES] {
        let mut out = [0u8; EVENT_BYTES];
        out[TIME_BYTES..TIME_BYTES + 2].copy_from_slice(&self.kind.to_le_bytes());
        out[TIME_BYTES + 2..TIME_BYTES + 4].copy_from_slice(&self.code.to_le_bytes());
        out[TIME_BYTES + 4..].copy_from_slice(&self.value.to_le_bytes());
        out
    }

    /// Reads the fields of `bytes`, ignoring the time.
    pub fn from_bytes(bytes: &[u8; EVENT_BYTES]) -> Self {
        let (_, tail) = bytes.split_at(TIME_BYTES);
        let kind = u16::from_le_bytes([tail[0], tail[1]]);
        let code = u16::from_le_bytes([tail[2], tail[3]]);
        let value = i32::from_le_bytes([tail[4], tail[5], tail[6], tail[7]]);
        Self { kind, code, value }
    }
}
