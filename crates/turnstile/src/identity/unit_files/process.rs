//! What the held process itself shows, read through the privilege the service keeps: whether its
//! environment carries a loader variable and whether anything is tracing it. A unit that unsets the
//! loader variables on paper is only as good as the environment its process actually got.

use peephole::ProcFs;
use peephole_core::{LOADER_VARIABLES, Pid};
use std::path::PathBuf;

/// The most of a process's environment that is read: the real ones are a few kilobytes, and a
/// larger one is refused rather than half-read.
pub const ENVIRON_CAP: u64 = 128 * 1024;
/// The most of a process's status file that is read.
pub const STATUS_CAP: u64 = 16 * 1024;

/// Why the process does not show what a real unit's process shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProcessFault {
    /// Its environment holds loader variables.
    #[error("its environment holds the loader variables {0}")]
    Loader(String),
    /// Another process traces it.
    #[error("another process is tracing it")]
    Traced,
    /// A file about it could not be read, or is not what that file looks like.
    #[error("{0} could not be read")]
    Unreadable(&'static str),
}

/// The loader variables named in `environ`, the NUL separated `NAME=value` block of a process.
pub fn loader_names(environ: &[u8]) -> Vec<&'static str> {
    LOADER_VARIABLES
        .iter()
        .copied()
        .filter(|name| {
            environ.split(|b| *b == 0).any(|entry| {
                entry
                    .strip_prefix(name.as_bytes())
                    .is_some_and(|rest| rest.first() == Some(&b'='))
            })
        })
        .collect()
}

/// The `TracerPid:` of a status file; `None` when there is no such line.
pub fn tracer_of(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("TracerPid:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Checks the process `pid` through `fs`: no loader variable in its environment, nobody tracing it.
pub fn check<P: ProcFs>(fs: &P, pid: Pid) -> Result<(), ProcessFault> {
    let environ = fs
        .read_capped(&PathBuf::from(format!("{pid}/environ")), ENVIRON_CAP)
        .map_err(|_| ProcessFault::Unreadable("its environment"))?;
    let names = loader_names(&environ);
    if !names.is_empty() {
        return Err(ProcessFault::Loader(names.join(" ")));
    }
    let status = fs
        .read_capped(&PathBuf::from(format!("{pid}/status")), STATUS_CAP)
        .map_err(|_| ProcessFault::Unreadable("its status"))?;
    let status = String::from_utf8(status).map_err(|_| ProcessFault::Unreadable("its status"))?;
    match tracer_of(&status) {
        Some(0) => Ok(()),
        Some(_) => Err(ProcessFault::Traced),
        None => Err(ProcessFault::Unreadable("its status")),
    }
}
