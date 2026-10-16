//! The socket systemd passes in. The service never binds a path of its own: the socket unit makes
//! `/run/turnstile/input.sock` with the right mode, so the permissions live in one place.

use listenfd::ListenFd;
use std::os::fd::OwnedFd;

/// Why there is no socket.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ActivationError {
    /// Started by hand, or by a unit without a socket.
    #[error("no socket was passed in: start turnstile through its socket unit")]
    NotActivated,
    /// The first descriptor is not a sequential-packet socket.
    #[error("the socket passed in is not a sequential-packet socket: {0}")]
    WrongSocket(std::io::Error),
}

/// Takes the first socket systemd passed in, checking it is a `SOCK_SEQPACKET` `AF_UNIX` socket and
/// marking it close-on-exec.
pub fn listener() -> Result<OwnedFd, ActivationError> {
    ListenFd::from_env()
        .take_custom::<OwnedFd>(
            0,
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET,
            "sequential-packet socket",
        )
        .map_err(ActivationError::WrongSocket)?
        .ok_or(ActivationError::NotActivated)
}
