//! Who is calling. The kernel names the process behind the socket (a pidfd, so a recycled process
//! id cannot pass for another program); `peephole` judges it against the root-owned table of
//! desktop programs; [`crate::policy`] decides what that program may have.

// Test seams: never in a shipped build.
#[cfg(any(test, feature = "fake"))]
pub mod fake;
pub mod peephole_authority;
pub mod seat;
pub mod unit_files;

use peephole::{LoadError, PinError, ProbeError};
use peephole_core::{Proof, RoleName};
use rustix::io::Errno;
use rustix::net::sockopt::socket_peercred;
use std::os::fd::OwnedFd;

/// What the table says about a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// A listed program of the desktop.
    Listed {
        /// Its role in the table (`pad_daemon`, `keymap_daemon`, ...).
        role: RoleName,
        /// How well it was proven (`Full` is the unit: its files are root-owned, it unsets the
        /// loader variables, and the process shows none of them).
        proof: Proof,
    },
    /// Not in the table, or not proven.
    Unlisted,
}

/// The facts about the process behind a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// Process id, for the log.
    pub pid: u32,
    /// The user the kernel vouches for.
    pub uid: u32,
    /// Its control group path, for example `/system.slice/keycap.service`.
    pub cgroup: String,
    /// Its image, for the log (`None` when the kernel hides it).
    pub image: Option<String>,
    /// What the table says.
    pub credential: Credential,
}

impl Caller {
    /// The log fields that name the caller.
    pub fn describe(&self) -> String {
        format!(
            "pid={} uid={} image={}",
            self.pid,
            self.uid,
            self.image.as_deref().unwrap_or("?")
        )
    }
}

/// Why a caller could not be identified.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentifyError {
    /// The kernel would not name the process (an old kernel, or it is gone).
    #[error("the caller could not be pinned: {0}")]
    Pin(#[source] PinError),
    /// Its facts could not be read.
    #[error("the caller's facts could not be read: {0}")]
    Facts(#[source] ProbeError),
    /// The table of desktop programs could not be read.
    #[error("the table of desktop programs could not be read: {0}")]
    Table(#[source] LoadError),
    /// The kernel would not name the user behind the socket.
    #[error("the user behind the socket could not be named: {0}")]
    Peer(#[source] Errno),
}

/// Names the process behind a connection.
pub trait Authority: Send + Sync + 'static {
    /// Identifies the peer of `conn`. It may take for ever: the peer controls what its own
    /// `/proc` entries and image file wait on, so the daemon calls it on a thread of its own.
    fn identify(&self, conn: &OwnedFd) -> Result<Caller, IdentifyError>;

    /// The user the kernel names for the peer of `conn`, from the socket alone: no `/proc` read, so
    /// it cannot wait on the peer. The daemon counts a connection by this before it identifies it.
    fn peer_uid(&self, conn: &OwnedFd) -> Result<u32, IdentifyError> {
        socket_peercred(conn)
            .map(|cred| cred.uid.as_raw())
            .map_err(IdentifyError::Peer)
    }
}

pub use seat::{ActiveUid, SeatSource};
