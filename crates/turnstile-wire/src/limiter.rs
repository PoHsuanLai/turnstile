//! The per-device rate limit on `Events` datagrams, pure: the caller passes the time.

use crate::limits::{EVENT_BURST, EVENT_DATAGRAMS_PER_SECOND, FLOOD_CUT_MS, FLOOD_GAP_MS};

/// Milliseconds on a clock the caller chooses; only differences are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Millis(pub u64);

impl Millis {
    /// Milliseconds from `earlier` to `self`, zero if `earlier` is later.
    pub const fn since(self, earlier: Millis) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// What to do with one datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Admit {
    /// Write it.
    Pass,
    /// Over the rate: drop it and count it.
    Drop,
    /// The excess has lasted [`FLOOD_CUT_MS`]: close the connection.
    Cut,
}

/// How long the current excess has lasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flood {
    Quiet,
    Since { start: Millis, last: Millis },
}

/// A token bucket of [`EVENT_BURST`] datagrams refilled at [`EVENT_DATAGRAMS_PER_SECOND`], plus the
/// count of how long the excess has gone on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limiter {
    /// Thousandths of a datagram, so the refill stays in whole numbers.
    tokens: u64,
    at: Millis,
    flood: Flood,
}

const MILLI: u64 = 1_000;

impl Limiter {
    /// A full bucket at `now`.
    pub const fn new(now: Millis) -> Self {
        Self {
            tokens: EVENT_BURST * MILLI,
            at: now,
            flood: Flood::Quiet,
        }
    }

    /// Decides one datagram arriving at `now`.
    pub fn admit(&mut self, now: Millis) -> Admit {
        let refill = now
            .since(self.at)
            .saturating_mul(EVENT_DATAGRAMS_PER_SECOND);
        self.tokens = self.tokens.saturating_add(refill).min(EVENT_BURST * MILLI);
        self.at = now;
        match self.tokens.checked_sub(MILLI) {
            Some(left) => {
                self.tokens = left;
                Admit::Pass
            }
            None => self.over(now),
        }
    }

    fn over(&mut self, now: Millis) -> Admit {
        let start = match self.flood {
            Flood::Since { start, last } if now.since(last) <= FLOOD_GAP_MS => start,
            Flood::Since { .. } | Flood::Quiet => now,
        };
        self.flood = Flood::Since { start, last: now };
        if now.since(start) >= FLOOD_CUT_MS {
            Admit::Cut
        } else {
            Admit::Drop
        }
    }
}
