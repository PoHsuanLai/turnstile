//! turnstile: the root service that makes palmrest's and keycap's virtual input devices, and no
//! others.
//!
//! systemd owns the socket and starts the service on the first connection; the service exits by
//! itself after a minute with no one connected. Each connection is one thread. A caller is
//! identified by the kernel (the process behind the socket, held by a pidfd), judged against the
//! root-owned table of desktop programs, and then may ask for exactly the device kinds of its own
//! program. palmrest's devices are proxied: every event passes the ceiling in `turnstile-wire`.
//! keycap's are made to a fixed shape and handed over as descriptors.
//!
//! The seams are traits with fakes: [`backend::Backend`] (the kernel), [`identity::Authority`] (who
//! is calling), [`identity::SeatSource`] (who is logged in) and [`clock::Clock`]. The tests drive
//! the whole service over a socket pair with the fakes and touch no device.
#![forbid(unsafe_code)]

pub mod activation;
pub mod backend;
pub mod clock;
pub mod daemon;
pub mod identity;
pub mod idle;
pub mod journal;
pub mod link;
pub mod policy;
pub mod serve;
pub mod session;

use backend::real::UinputBackend;
use clock::Monotonic;
use daemon::{Daemon, DaemonError, Settings};
use identity::peephole_authority::PeepholeAuthority;
use identity::seat::SeatFile;
use serve::Context;

/// Why the service stopped with an error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// It was not started by its socket unit.
    #[error("{0}")]
    Activation(#[from] activation::ActivationError),
    /// The accept loop failed.
    #[error("{0}")]
    Daemon(#[from] DaemonError),
}

/// Runs the service on the socket systemd passed in, until it has been idle for a minute.
pub fn run() -> Result<(), RunError> {
    let listener = activation::listener()?;
    let context = Context {
        backend: UinputBackend::default(),
        authority: PeepholeAuthority::system(),
        seat: SeatFile::system(),
        clock: Monotonic::new(),
    };
    Daemon::new(listener, context, Settings::default()).run()?;
    Ok(())
}
