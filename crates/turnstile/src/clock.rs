//! Time as an argument: the service reads the clock in one place.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use turnstile_wire::Millis;

/// A source of monotonic milliseconds.
pub trait Clock: Send + Sync + 'static {
    /// Milliseconds since some fixed moment; never goes back.
    fn now(&self) -> Millis;
}

/// The real clock, counting from when it was made.
#[derive(Debug, Clone, Copy)]
pub struct Monotonic {
    start: Instant,
}

impl Monotonic {
    /// A clock that reads zero now.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for Monotonic {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for Monotonic {
    fn now(&self) -> Millis {
        Millis(u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX))
    }
}

/// A clock a test moves by hand.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: AtomicU64,
}

impl ManualClock {
    /// A clock at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the clock forward by `ms`.
    pub fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Millis {
        Millis(self.now.load(Ordering::SeqCst))
    }
}
