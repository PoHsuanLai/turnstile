//! A kernel that records. The tests of the service, and of anything built on it, use this and make
//! no device.

use super::{Backend, BackendError, Device};
use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
use std::io;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use turnstile_wire::{DeviceSpec, Event};

/// What happened to a fake device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// It exists.
    Alive,
    /// Its owner dropped it: the kernel would have destroyed it.
    Destroyed,
    /// Its descriptor went to a client.
    Handed,
}

/// Everything the fake kernel was asked to do.
#[derive(Debug, Default)]
pub struct FakeLog {
    /// Every device made, in order; the index is the device's number.
    pub specs: Vec<DeviceSpec>,
    /// What became of each.
    pub fates: Vec<Fate>,
    /// Every write: the device's number and the events.
    pub writes: Vec<(usize, Vec<Event>)>,
    /// The kernel's end of each handed descriptor: what the client writes arrives here.
    pub handed_peers: Vec<OwnedFd>,
    /// The next `create` fails.
    pub fail_create: Option<io::ErrorKind>,
    /// The next `write` fails.
    pub fail_write: Option<io::ErrorKind>,
}

/// The fake kernel. Clone it to keep a handle on its log.
#[derive(Debug, Clone, Default)]
pub struct FakeBackend {
    log: Arc<Mutex<FakeLog>>,
}

impl FakeBackend {
    /// An empty fake kernel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads or changes the log.
    pub fn log(&self) -> MutexGuard<'_, FakeLog> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Backend for FakeBackend {
    type Device = FakeDevice;

    fn create(&self, spec: &DeviceSpec) -> Result<FakeDevice, BackendError> {
        let mut log = self.log();
        if let Some(kind) = log.fail_create.take() {
            return Err(BackendError::Create(io::Error::new(kind, "injected")));
        }
        let index = log.specs.len();
        log.specs.push(spec.clone());
        log.fates.push(Fate::Alive);
        Ok(FakeDevice {
            index,
            backend: self.clone(),
        })
    }
}

/// A device on the fake kernel.
#[derive(Debug)]
pub struct FakeDevice {
    index: usize,
    backend: FakeBackend,
}

impl Device for FakeDevice {
    fn write(&mut self, events: &[Event]) -> Result<(), BackendError> {
        let mut log = self.backend.log();
        if let Some(kind) = log.fail_write.take() {
            return Err(BackendError::Write(io::Error::new(kind, "injected")));
        }
        log.writes.push((self.index, events.to_vec()));
        Ok(())
    }

    fn into_fd(self) -> OwnedFd {
        let (ours, theirs) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .expect("a socket pair for the fake descriptor");
        let mut log = self.backend.log();
        log.handed_peers.push(theirs);
        log.fates[self.index] = Fate::Handed;
        drop(log);
        ours
    }
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        let mut log = self.backend.log();
        if let Some(fate @ Fate::Alive) = log.fates.get_mut(self.index) {
            *fate = Fate::Destroyed;
        }
    }
}
