//! Who is calling. The kernel names the process behind the socket (a pidfd, so a recycled process
//! id cannot pass for another program); `peephole` judges it against the root-owned table of
//! desktop programs; [`crate::policy`] decides what that program may have.

pub mod fake;
pub mod peephole_authority;
pub mod seat;

use peephole_core::RoleName;
use std::os::fd::OwnedFd;

/// How well a caller was proven to be the listed program, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    /// Only the control group it runs in.
    Cgroup,
    /// And its image: a root-owned file that is in the table.
    Image,
    /// And the unit manager's own facts.
    Manager,
}

/// What the table says about a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// A listed program of the desktop.
    Listed {
        /// Its role in the table (`pad_daemon`, `keymap_daemon`, ...).
        role: RoleName,
        /// How well it was proven.
        strength: Strength,
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
    Pin(String),
    /// Its facts could not be read.
    #[error("the caller's facts could not be read: {0}")]
    Facts(String),
    /// The table of desktop programs could not be read.
    #[error("the table of desktop programs could not be read: {0}")]
    Table(String),
}

/// Names the process behind a connection.
pub trait Authority: Send + Sync + 'static {
    /// Identifies the peer of `conn`.
    fn identify(&self, conn: &OwnedFd) -> Result<Caller, IdentifyError>;
}

pub use seat::{ActiveUid, SeatSource};
