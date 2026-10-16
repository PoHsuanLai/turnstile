//! When the service has nothing left to do. Pure: the caller passes the time and the count.

use turnstile_wire::Millis;

/// What to do about the quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Idling {
    /// Someone is connected, or it has not been quiet long enough.
    Stay,
    /// Quiet for the whole limit: exit; socket activation starts the service again.
    Exit,
}

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

    /// Looks at the connection count at `now`.
    pub fn observe(&mut self, now: Millis, connections: usize) -> Idling {
        if connections > 0 {
            self.quiet_since = None;
            return Idling::Stay;
        }
        let since = *self.quiet_since.get_or_insert(now);
        if now.since(since) >= self.limit_ms {
            Idling::Exit
        } else {
            Idling::Stay
        }
    }
}
