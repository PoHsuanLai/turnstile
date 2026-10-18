//! A count per key and a total, the one counter behind [`crate::slots`] and [`crate::pending`].

use std::collections::HashMap;
use std::hash::Hash;

/// How many of each key are held, and how many in all.
#[derive(Debug)]
pub(crate) struct Tally<K> {
    per_key: HashMap<K, usize>,
    total: usize,
}

impl<K> Default for Tally<K> {
    fn default() -> Self {
        Self {
            per_key: HashMap::new(),
            total: 0,
        }
    }
}

impl<K: Hash + Eq + Copy> Tally<K> {
    /// How many `key` holds.
    pub(crate) fn count(&self, key: K) -> usize {
        self.per_key.get(&key).copied().unwrap_or(0)
    }

    /// How many are held, of every key.
    pub(crate) fn total(&self) -> usize {
        self.total
    }

    /// One more of `key`.
    pub(crate) fn add(&mut self, key: K) {
        *self.per_key.entry(key).or_default() += 1;
        self.total += 1;
    }

    /// One fewer of `key`, never below none; a key at none is forgotten.
    pub(crate) fn sub(&mut self, key: K) {
        let left = self.count(key).saturating_sub(1);
        if left == 0 {
            self.per_key.remove(&key);
        } else {
            self.per_key.insert(key, left);
        }
        self.total = self.total.saturating_sub(1);
    }
}
