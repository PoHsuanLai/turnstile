//! When the service has nothing left to do. Pure: the caller passes the time and the count.

use turnstile_wire::Millis;

/// Counts how long no one has been connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idle {
    limit_ms: u64,
    quiet_since: Option<Millis>,
}

impl Idle {
    /// Exit after `limit_ms` with no connection.
    pub const fn new(limit_ms: u64) -> Self {
        Self {
            limit_ms,
            quiet_since: None,
        }
    }

    /// Looks at the connection count at `now`. True once it has been quiet for the whole limit:
    /// the service exits and socket activation starts it again.
    pub fn observe(&mut self, now: Millis, connections: usize) -> bool {
        if connections > 0 {
            self.quiet_since = None;
            return false;
        }
        let since = *self.quiet_since.get_or_insert(now);
        now.since(since) >= self.limit_ms
    }
}
