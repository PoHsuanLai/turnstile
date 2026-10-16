//! The real authority: `peephole` pins the process behind the socket, probes its facts and judges
//! them against the table. This is the one file that knows `peephole`'s types; everything else
//! sees a [`Caller`].
//!
//! The service holds `CAP_SYS_PTRACE` so that the probe can read `/proc/<pid>/exe` of another user's
//! process (palmrest). It never asks a unit manager: that would mean connecting to a socket the user
//! controls from a root process, so the proof stops at the image (`Proof::Exe`).

use super::{Authority, Caller, Credential, IdentifyError, Strength};
use peephole::{Pinned, Prober, ProcFs, RealProc, TableFiles};
use peephole_core::{Exe, Proof, Standing, judge};
use std::os::fd::OwnedFd;

/// Judges callers against the table on disk, through a view of `/proc` and the file system.
#[derive(Debug)]
pub struct PeepholeAuthority<P = RealProc> {
    prober: Prober<P>,
    files: TableFiles,
}

impl PeepholeAuthority<RealProc> {
    /// The authority of this machine: the real `/proc` and the shipped table.
    pub fn system() -> Self {
        Self::new(Prober::system(), TableFiles::desktop())
    }
}

impl<P: ProcFs> PeepholeAuthority<P> {
    /// An authority reading through `prober`, with the table in `files`; the table is read through
    /// the same view, so a test's fixture tree stands for both.
    pub fn new(prober: Prober<P>, files: TableFiles) -> Self {
        Self { prober, files }
    }
}

impl<P: ProcFs + 'static> Authority for PeepholeAuthority<P> {
    fn identify(&self, conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        let pinned = Pinned::of_stream(conn).map_err(|e| IdentifyError::Pin(e.to_string()))?;
        let facts = self
            .prober
            .probe(&pinned, None)
            .map_err(|e| IdentifyError::Facts(e.to_string()))?;
        // The table is read after the process is held, and again for every connection: a few small
        // files, and a change of the table takes effect at the next connection.
        let table = self
            .files
            .load(self.prober.fs())
            .map_err(|e| IdentifyError::Table(e.to_string()))?;
        let standing = judge(&facts, &table);
        Ok(Caller {
            pid: facts.pid.get(),
            uid: facts.uid.get(),
            cgroup: facts.cgroup.as_str().to_owned(),
            image: match &facts.exe {
                Exe::Read { path, .. } => Some(path.to_text()),
                _ => None,
            },
            credential: credential(&standing),
        })
    }
}

/// What a standing means to the service: a listed program with a role and a proof, or nothing.
fn credential(standing: &Standing) -> Credential {
    match standing {
        Standing::Service { role, proof, .. } => Credential::Listed {
            role: role.clone(),
            strength: match proof {
                Proof::Exe => Strength::Image,
                Proof::Full => Strength::Manager,
                _ => Strength::Cgroup,
            },
        },
        _ => Credential::Unlisted,
    }
}
