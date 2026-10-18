//! What the held process itself shows, read through the privilege the service keeps: whether
//! anything is tracing it.
//!
//! Its environment is not read (NEW-FA-1). `/proc/<pid>/environ` is mode 0400 and owned by the
//! process's user, and the kernel checks that mode before ptrace access, so root holding only
//! `CAP_SYS_PTRACE` cannot open it for palmrest's or keycap's dynamic user. Only root can set a
//! system unit's environment, and the unit's root-owned files must unset every loader variable
//! (`super::directives`); the capability that would open the file would let a compromised service
//! read every file, and reading the process's memory instead would let it read every process's.

use peephole::ProcFs;
use peephole_core::Pid;
/// The most of a process's status file that is read.
pub const STATUS_CAP: u64 = 16 * 1024;

/// Why the process does not show what a real unit's process shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProcessFault {
    /// Another process traces it.
    #[error("another process is tracing it")]
    Traced,
    /// A file about it could not be read, or is not what that file looks like.
    #[error("{0} could not be read")]
    Unreadable(&'static str),
}

/// The `TracerPid:` of a status file; `None` when there is no such line.
pub fn tracer_of(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("TracerPid:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Checks the process `pid` through `fs`: nobody is tracing it.
pub fn check<P: ProcFs>(fs: &P, pid: Pid) -> Result<(), ProcessFault> {
    let status = fs
        .read_capped(
            &fs.proc_root().join(pid.to_string()).join("status"),
            STATUS_CAP,
        )
        .map_err(|_| ProcessFault::Unreadable("its status"))?;
    let status = String::from_utf8(status).map_err(|_| ProcessFault::Unreadable("its status"))?;
    match tracer_of(&status) {
        Some(0) => Ok(()),
        Some(_) => Err(ProcessFault::Traced),
        None => Err(ProcessFault::Unreadable("its status")),
    }
}
