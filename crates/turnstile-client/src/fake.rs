//! A stand-in for the service, for the tests of palmrest, keycap and this crate: no socket, no
//! daemon, no device. It records what a caller asked for and answers as it is told to.

use crate::error::ClientError;
use crate::session::{DeviceSource, Made, Sent, Session};
use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use turnstile_wire::{Create, Delivery, DeviceId, Event, Reason, plan};

/// What the fake does with `send`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SendMode {
    /// Record the events.
    #[default]
    Accept,
    /// Answer [`Sent::Dropped`], as a service with a full queue does.
    Drop,
    /// Answer [`ClientError::Closed`], as a service that cut the connection does.
    Close,
}

/// What the fake does with `open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OpenMode {
    /// Connect.
    #[default]
    Work,
    /// Answer [`ClientError::Absent`].
    Absent,
}

/// Everything a caller did, in order.
#[derive(Debug, Default)]
pub struct FakeState {
    /// How many sessions were opened.
    pub opened: u32,
    /// Every `Create`, including the refused ones.
    pub created: Vec<Create>,
    /// Every event datagram accepted, with its device.
    pub sent: Vec<(DeviceId, Vec<Event>)>,
    /// Every `destroy`.
    pub destroyed: Vec<DeviceId>,
    /// The next `Create` answers `Refused` with this.
    pub refuse_next: Option<Reason>,
    /// What `send` does.
    pub send_mode: SendMode,
    /// What `open` does.
    pub open_mode: OpenMode,
    /// The service's end of each handed descriptor: what the caller writes to its fd arrives here.
    pub handed_peers: Vec<OwnedFd>,
    next_id: u8,
}

/// The fake service. Clone it to keep a handle on what the caller did.
#[derive(Debug, Clone, Default)]
pub struct FakeSource {
    state: Arc<Mutex<FakeState>>,
}

impl FakeSource {
    /// A working fake.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads or changes the record.
    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DeviceSource for FakeSource {
    type Session = FakeSession;

    fn open(&self) -> Result<FakeSession, ClientError> {
        let mut state = self.state();
        match state.open_mode {
            OpenMode::Absent => Err(ClientError::Absent),
            OpenMode::Work => {
                state.opened += 1;
                Ok(FakeSession {
                    source: self.clone(),
                })
            }
        }
    }
}

/// A session on the fake.
#[derive(Debug)]
pub struct FakeSession {
    source: FakeSource,
}

impl Session for FakeSession {
    fn create(&mut self, request: &Create) -> Result<Made, ClientError> {
        let mut state = self.source.state();
        state.created.push(request.clone());
        if let Some(reason) = state.refuse_next.take() {
            return Err(ClientError::Refused(reason));
        }
        // The same ceiling the real service applies.
        plan(request)
            .map(drop)
            .map_err(|_| ClientError::Refused(Reason::BadCaps))?;
        let device = DeviceId(state.next_id);
        state.next_id = state.next_id.wrapping_add(1);
        match request.kind.delivery() {
            Delivery::Handed => {
                let (ours, theirs) = socketpair(
                    AddressFamily::UNIX,
                    SocketType::SEQPACKET,
                    SocketFlags::CLOEXEC,
                    None,
                )
                .map_err(|e| ClientError::Io(e.into()))?;
                state.handed_peers.push(theirs);
                Ok(Made::Handed { device, fd: ours })
            }
            _ => Ok(Made::Proxied(device)),
        }
    }

    fn send(&mut self, device: DeviceId, events: &[Event]) -> Result<Sent, ClientError> {
        let mut state = self.source.state();
        match state.send_mode {
            SendMode::Accept => {
                state.sent.push((device, events.to_vec()));
                Ok(Sent::Queued)
            }
            SendMode::Drop => Ok(Sent::Dropped),
            SendMode::Close => Err(ClientError::Closed),
        }
    }

    fn destroy(&mut self, device: DeviceId) -> Result<(), ClientError> {
        self.source.state().destroyed.push(device);
        Ok(())
    }

    fn refusal(&mut self) -> Option<Reason> {
        None
    }
}
