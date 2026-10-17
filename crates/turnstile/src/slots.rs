//! Who may hold a connection (NEW-PI-6). The socket is open to every local account, so slots cannot
//! be first come: an account that opened sixteen idle connections would keep palmrest and keycap
//! from reaching the service at all.
//!
//! A peer is identified first, then counted. A caller shown to be palmrest or keycap holds a slot of
//! its own reserve, outside every other cap. Anyone else holds a slot of its user's, at most
//! [`MAX_CONNECTIONS_PER_USER`], and of the shared total for strangers, [`MAX_CONNECTIONS`].

use crate::identity::{ActiveUid, Caller};
use crate::policy::authorise;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use turnstile_wire::Kind;
use turnstile_wire::limits::{
    HANDSHAKE_TIMEOUT_MS, MAX_CONNECTIONS, MAX_CONNECTIONS_PER_USER, MAX_RESERVED_PER_CLIENT,
    STRANGER_HANDSHAKE_TIMEOUT_MS,
};

/// One of the two programs the service exists for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Client {
    /// The touchpad daemon.
    Palmrest,
    /// The keymap daemon.
    Keycap,
}

/// Which pool of slots a connection counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// A caller that passes the policy for one client's devices.
    Reserved(Client),
    /// Anyone else, by the user the kernel names.
    Stranger(u32),
}

impl Bucket {
    /// The pool for `caller` while `seat`'s user is active.
    pub fn of(caller: &Caller, seat: ActiveUid) -> Self {
        if authorise(Kind::PalmrestWheel, caller, seat).is_ok() {
            Self::Reserved(Client::Palmrest)
        } else if authorise(Kind::KeycapKeyboard, caller, seat).is_ok() {
            Self::Reserved(Client::Keycap)
        } else {
            Self::Stranger(caller.uid)
        }
    }

    /// How long a connection of this pool may stay without making a device.
    pub fn handshake_ms(self) -> u64 {
        match self {
            Self::Reserved(_) => HANDSHAKE_TIMEOUT_MS,
            Self::Stranger(_) => STRANGER_HANDSHAKE_TIMEOUT_MS,
        }
    }
}

/// No slot is free in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no free connection slot")]
pub struct NoSlot;

#[derive(Debug, Default)]
struct Counts {
    reserved: HashMap<Client, usize>,
    per_user: HashMap<u32, usize>,
    strangers: usize,
    total: usize,
}

/// The live connections, by pool. Cloning shares the counts.
#[derive(Debug, Clone, Default)]
pub struct Slots(Arc<Mutex<Counts>>);

impl Slots {
    /// No connection yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A slot in `bucket`, held until the returned token drops.
    pub fn take(&self, bucket: Bucket) -> Result<Slot, NoSlot> {
        let mut counts = self.counts();
        match bucket {
            Bucket::Reserved(client) => {
                let held = counts.reserved.entry(client).or_default();
                if *held >= MAX_RESERVED_PER_CLIENT {
                    return Err(NoSlot);
                }
                *held += 1;
            }
            Bucket::Stranger(uid) => {
                let mine = counts.per_user.get(&uid).copied().unwrap_or(0);
                if mine >= MAX_CONNECTIONS_PER_USER || counts.strangers >= MAX_CONNECTIONS {
                    return Err(NoSlot);
                }
                counts.per_user.insert(uid, mine + 1);
                counts.strangers += 1;
            }
        }
        counts.total += 1;
        drop(counts);
        Ok(Slot {
            slots: self.clone(),
            bucket,
        })
    }

    /// Connections held now, in every pool.
    pub fn open(&self) -> usize {
        self.counts().total
    }

    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn release(&self, bucket: Bucket) {
        let mut counts = self.counts();
        match bucket {
            Bucket::Reserved(client) => {
                if let Some(held) = counts.reserved.get_mut(&client) {
                    *held = held.saturating_sub(1);
                }
            }
            Bucket::Stranger(uid) => {
                let left = counts
                    .per_user
                    .get(&uid)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(1);
                if left == 0 {
                    counts.per_user.remove(&uid);
                } else {
                    counts.per_user.insert(uid, left);
                }
                counts.strangers = counts.strangers.saturating_sub(1);
            }
        }
        counts.total = counts.total.saturating_sub(1);
    }
}

/// A held slot; it is given back when this drops, however the connection's thread ends.
#[derive(Debug)]
pub struct Slot {
    slots: Slots,
    bucket: Bucket,
}

impl Slot {
    /// The pool this slot belongs to.
    pub fn bucket(&self) -> Bucket {
        self.bucket
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.slots.release(self.bucket);
    }
}
