//! Who is logged in on the seat: the `ACTIVE_UID` line of logind's file, the same fact keycap reads.

use peephole_core::Uid;
use std::io::Read;
use std::path::PathBuf;

/// Where logind records the seat's state.
pub const SEAT_FILE: &str = "/run/systemd/seats/seat0";
/// The most of the seat file that is read: it holds a few short lines.
const MAX_SEAT_BYTES: u64 = 4096;

/// The user of the active session, if there is one (there is none before anyone logs in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveUid(pub Option<Uid>);

/// Where the active user comes from.
pub trait SeatSource: Send + Sync + 'static {
    /// The user of the active session now.
    fn active_uid(&self) -> ActiveUid;
}

/// Reads logind's seat file each time it is asked: a login changes it.
#[derive(Debug, Clone)]
pub struct SeatFile {
    path: PathBuf,
}

impl SeatFile {
    /// The seat file of this machine.
    pub fn system() -> Self {
        Self::at(SEAT_FILE)
    }

    /// A seat file somewhere else, for a test.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SeatSource for SeatFile {
    fn active_uid(&self) -> ActiveUid {
        let mut text = String::new();
        let read = std::fs::File::open(&self.path)
            .and_then(|f| f.take(MAX_SEAT_BYTES).read_to_string(&mut text));
        match read {
            Ok(_) => parse_active_uid(&text),
            Err(_) => ActiveUid(None),
        }
    }
}

/// The `ACTIVE_UID=` line of a seat file; none when there is no active session.
pub fn parse_active_uid(text: &str) -> ActiveUid {
    ActiveUid(
        text.lines()
            .find_map(|line| line.strip_prefix("ACTIVE_UID="))
            .and_then(|value| value.trim().parse().ok())
            .map(Uid::new),
    )
}

/// A seat a test sets by hand.
#[derive(Debug, Clone, Copy)]
pub struct FixedSeat(pub ActiveUid);

impl SeatSource for FixedSeat {
    fn active_uid(&self) -> ActiveUid {
        self.0
    }
}
