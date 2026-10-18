//! Who is being identified right now (NEW-FA-3). Naming a peer reads its `/proc` entries and stats
//! its image file, and a program that controls its own file system can make that wait for ever. So
//! each connection is identified on a thread of its own, never on the accept loop, and what is
//! waiting is counted here, before the thread exists, by the user the kernel names from the socket
//! alone (no `/proc` read).
//!
//! A user over [`Caps::per_user`], or all users over [`Caps::total`], is closed at once. Identified
//! callers are still counted into [`crate::slots`] afterwards (NEW-PI-6), so strangers cannot use
//! the reserve of palmrest and keycap. What a stuck identification costs is bounded: its user's
//! share of these caps, and nothing of the accept loop. The caps are sized so the threads they
//! allow plus the slots' fit the unit's `TasksMax=` (checked at compile time in the limits).

use crate::tally::Tally;
use peephole_core::Uid;
use std::sync::{Arc, Mutex, PoisonError};
use turnstile_wire::limits::{MAX_PENDING_IDENTIFICATIONS, MAX_PENDING_PER_USER};

/// How many identifications may wait at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// From one user.
    pub per_user: usize,
    /// From every user together.
    pub total: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            per_user: MAX_PENDING_PER_USER,
            total: MAX_PENDING_IDENTIFICATIONS,
        }
    }
}

/// Which cap was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Crowded {
    /// The user has as many identifications waiting as it may.
    #[error("too many connections from this user are being identified")]
    User,
    /// Every user together do.
    #[error("too many connections are being identified")]
    All,
}

#[derive(Debug, Default)]
struct Counts(Tally<Uid>);

/// The identifications waiting now. Cloning shares the counts.
#[derive(Debug, Clone)]
pub struct Pending {
    caps: Caps,
    counts: Arc<Mutex<Counts>>,
}

impl Pending {
    /// Nothing waiting yet.
    pub fn new(caps: Caps) -> Self {
        Self {
            caps,
            counts: Arc::default(),
        }
    }

    /// A place for one identification of a connection from `uid`, held until the ticket drops.
    pub fn enter(&self, uid: Uid) -> Result<Ticket, Crowded> {
        let mut counts = self.counts();
        if counts.0.count(uid) >= self.caps.per_user {
            return Err(Crowded::User);
        }
        if counts.0.total() >= self.caps.total {
            return Err(Crowded::All);
        }
        counts.0.add(uid);
        drop(counts);
        Ok(Ticket {
            pending: self.clone(),
            uid,
        })
    }

    /// Identifications waiting now, from every user.
    pub fn count(&self) -> usize {
        self.counts().0.total()
    }

    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn release(&self, uid: Uid) {
        self.counts().0.sub(uid);
    }
}

/// A place in the queue; it is given back when this drops, however the identification ends.
#[derive(Debug)]
pub struct Ticket {
    pending: Pending,
    uid: Uid,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.pending.release(self.uid);
    }
}
