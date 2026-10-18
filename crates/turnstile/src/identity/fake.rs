//! An authority that names every caller the same: for tests that drive the service over a socket
//! pair, where no real process stands behind the other end.

use super::{Authority, Caller, IdentifyError};
use peephole::PinError;
use std::os::fd::OwnedFd;

/// Names every peer as `caller`, or cannot pin any (the process is gone).
#[derive(Debug, Clone)]
pub struct FixedAuthority {
    answer: Option<Caller>,
}

impl FixedAuthority {
    /// Every peer is `caller`.
    pub fn naming(caller: Caller) -> Self {
        Self {
            answer: Some(caller),
        }
    }

    /// No peer can be identified.
    pub fn failing() -> Self {
        Self { answer: None }
    }
}

impl Authority for FixedAuthority {
    fn identify(&self, _conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        self.answer
            .clone()
            .ok_or(IdentifyError::Pin(PinError::PeerGone))
    }
}
