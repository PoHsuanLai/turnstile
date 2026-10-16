//! An authority that names every caller the same: for tests that drive the service over a socket
//! pair, where no real process stands behind the other end.

use super::{Authority, Caller, IdentifyError};
use std::os::fd::OwnedFd;

/// Names every peer as `caller`, or fails with `message`.
#[derive(Debug, Clone)]
pub struct FixedAuthority {
    answer: Result<Caller, String>,
}

impl FixedAuthority {
    /// Every peer is `caller`.
    pub fn naming(caller: Caller) -> Self {
        Self { answer: Ok(caller) }
    }

    /// No peer can be identified.
    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            answer: Err(message.into()),
        }
    }
}

impl Authority for FixedAuthority {
    fn identify(&self, _conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        self.answer.clone().map_err(IdentifyError::Pin)
    }
}
