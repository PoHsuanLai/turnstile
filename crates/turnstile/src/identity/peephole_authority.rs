//! The real authority: `peephole` pins the process behind the socket, probes its facts and judges
//! them against the table. This is the one file that knows `peephole`'s types; everything else
//! sees a [`Caller`].
//!
//! The service holds `CAP_SYS_PTRACE` so that the probe can read `/proc/<pid>/exe` and `status` of
//! another user's process. It never asks a unit manager (that would mean connecting to a socket the
//! user controls from a root process), so the unit's facts come from the unit files and the
//! process itself ([`super::unit_files`]). A listed program whose image checks out but whose unit
//! does not stays at `Proof::Exe`, and the policy refuses it.

use super::{Authority, Caller, Credential, IdentifyError, unit_files};
use crate::journal::{self, kv};
use peephole::{Pinned, Prober, ProcFs, RealProc, TableFiles};
use peephole_core::{Exe, Facts, Proof, Standing, Table, judge};
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
        let pinned = Pinned::of_stream(conn).map_err(IdentifyError::Pin)?;
        let facts = self
            .prober
            .probe(&pinned, None)
            .map_err(IdentifyError::Facts)?;
        // The table is read after the process is held, and again for every connection: a few small
        // files, and a change of the table takes effect at the next connection.
        let table = self
            .files
            .load(self.prober.fs())
            .map_err(IdentifyError::Table)?;
        let (facts, standing) = self.prove_unit(&pinned, facts, &table);
        Ok(Caller {
            pid: facts.pid,
            uid: facts.uid,
            cgroup: facts.cgroup,
            image: match facts.exe {
                Exe::Read { path, .. } => Some(path),
                _ => None,
            },
            credential: credential(&standing),
        })
    }
}

impl<P: ProcFs> PeepholeAuthority<P> {
    /// Judges `facts`; a listed service whose image checked out is judged again with the facts of
    /// its unit, which is the only way to `Proof::Full`. When the unit cannot be shown the first
    /// judgement stands (the policy refuses it) and the reason goes to the journal.
    fn prove_unit(&self, pinned: &Pinned, facts: Facts, table: &Table) -> (Facts, Standing) {
        let standing = judge(&facts, table);
        let Standing::Service {
            proof: Proof::Exe, ..
        } = standing
        else {
            return (facts, standing);
        };
        match unit_files::facts(self.prober.fs(), facts.pid, &facts.cgroup) {
            Ok(unit) => match self.prober.probe(pinned, Some(unit)) {
                Ok(again) => {
                    let judged = judge(&again, table);
                    (again, judged)
                }
                Err(why) => {
                    note_unproven(&facts, &why);
                    (facts, standing)
                }
            },
            Err(why) => {
                note_unproven(&facts, &why);
                (facts, standing)
            }
        }
    }
}

/// Says in the journal why a listed program was not shown to be its unit.
fn note_unproven(facts: &Facts, why: &dyn std::fmt::Display) {
    journal::note(
        "unproven",
        &[
            kv("pid", &facts.pid),
            kv("uid", &facts.uid),
            kv("cgroup", &facts.cgroup.as_str()),
            kv("why", &why),
        ],
    );
}

/// What a standing means to the service: a listed program with a role and a proof, or nothing.
fn credential(standing: &Standing) -> Credential {
    match standing {
        Standing::Service { role, proof, .. } => Credential::Listed {
            role: role.clone(),
            proof: *proof,
        },
        _ => Credential::Unlisted,
    }
}
