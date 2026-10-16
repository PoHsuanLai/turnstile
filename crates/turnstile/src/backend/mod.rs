//! The kernel's side: making a uinput device and writing events to it. A trait so the service is
//! tested without a device; [`real`] is the one implementation that touches `/dev/uinput`.

pub mod fake;
pub mod real;

use std::io;
use std::os::fd::OwnedFd;
use turnstile_wire::{DeviceSpec, Event};

/// Why the kernel side failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackendError {
    /// The device could not be set up or created.
    #[error("the device could not be made: {0}")]
    Create(io::Error),
    /// A write to the device failed.
    #[error("writing to the device failed: {0}")]
    Write(io::Error),
}

/// Makes devices.
pub trait Backend: Send + Sync + 'static {
    /// What it makes.
    type Device: Device;

    /// Makes the device `spec` describes. Nothing in `spec` came from a client without passing the
    /// ceiling first.
    fn create(&self, spec: &DeviceSpec) -> Result<Self::Device, BackendError>;
}

/// A device that exists. Dropping it destroys it.
pub trait Device: Send + 'static {
    /// Writes `events` in order.
    fn write(&mut self, events: &[Event]) -> Result<(), BackendError>;

    /// Gives up the device as its descriptor: it lives as long as the descriptor does.
    fn into_fd(self) -> OwnedFd;
}
