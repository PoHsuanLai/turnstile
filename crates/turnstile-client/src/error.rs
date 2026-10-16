//! Why a request to the service failed.

use std::io;
use turnstile_wire::{Reason, WireError};

/// Why a request to the service failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// There is no service to talk to: no socket, or nothing listening on it. The caller may use
    /// its own fallback.
    #[error("the input service is not running")]
    Absent,
    /// The service has its connection queue full.
    #[error("the input service is busy")]
    Busy,
    /// The service closed the connection.
    #[error("the input service closed the connection")]
    Closed,
    /// The service did not answer in time.
    #[error("the input service did not answer in time")]
    Timeout,
    /// The service refused the request.
    #[error("the input service refused the request: {0:?}")]
    Refused(Reason),
    /// The service sent a message that does not parse.
    #[error("the input service sent a message that does not parse: {0}")]
    Wire(#[from] WireError),
    /// The service sent a message this client does not expect at this point.
    #[error("the input service sent an unexpected message")]
    Unexpected,
    /// A handed device arrived without its file descriptor.
    #[error("the input service did not send the device")]
    NoDevice,
    /// A system call failed.
    #[error("talking to the input service failed: {0}")]
    Io(#[from] io::Error),
}
