//! One connection's state machine: requests in, replies out, devices owned. No socket and no clock
//! of its own, so it is driven by tests with a fake kernel and a hand-moved time.

use crate::backend::{Backend, Device};
use crate::policy::Admission;
use std::os::fd::OwnedFd;
use turnstile_wire::limits::{
    MAX_CREATES_PER_CONNECTION, MAX_DENIED_EVENTS, MAX_DEVICES_PER_CONNECTION,
    MAX_HANDED_PER_CONNECTION,
};
use turnstile_wire::{
    Admit, Create, Delivery, DeviceId, Event, Filter, Kind, Limiter, Millis, Reason, Reply,
    Request, Route, plan,
};

/// A reply, with the descriptor that rides with it when the device is handed over.
#[derive(Debug)]
pub struct Outgoing {
    /// The reply.
    pub reply: Reply,
    /// The device's descriptor, sent with a `Created` for a handed device.
    pub fd: Option<OwnedFd>,
}

/// Why a peer was refused, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The kind asked for, when the refusal was about one.
    pub kind: Option<Kind>,
    /// What was wrong, in a few plain words.
    pub why: String,
}

/// The peer was refused: send it `Refused(reason)`, log the note and close the connection.
#[derive(Debug)]
pub struct Refused {
    /// What the peer is told.
    pub reason: Reason,
    /// What goes to the log.
    pub note: Note,
}

impl Refused {
    fn new(reason: Reason, kind: Option<Kind>, why: impl ToString) -> Self {
        Self {
            reason,
            note: Note {
                kind,
                why: why.to_string(),
            },
        }
    }
}

/// What a request led to: keep reading after sending the reply if there is one, or refuse the peer.
pub type Handled = Result<Option<Outgoing>, Refused>;

/// What a connection cost, for the log when it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// `Events` datagrams dropped for exceeding the rate.
    pub dropped_datagrams: u64,
    /// Events a ceiling refused.
    pub denied_events: u32,
}

struct Slot<D> {
    id: DeviceId,
    device: D,
    filter: Filter,
    limiter: Limiter,
}

/// The state of one connection.
pub struct Session<'a, B: Backend, A: Admission> {
    backend: &'a B,
    admission: &'a A,
    slots: Vec<Slot<B::Device>>,
    created: u8,
    handed: u8,
    stats: Stats,
}

impl<'a, B: Backend, A: Admission> Session<'a, B, A> {
    /// A connection with nothing made yet.
    pub fn new(backend: &'a B, admission: &'a A) -> Self {
        Self {
            backend,
            admission,
            slots: Vec::new(),
            created: 0,
            handed: 0,
            stats: Stats::default(),
        }
    }

    /// True until a device is made: the peer has a short while to ask for one, and may then be
    /// silent for as long as it likes.
    pub fn in_handshake(&self) -> bool {
        self.created == 0
    }

    /// What the connection has cost so far.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Devices the service still holds (handed ones are the client's now).
    pub fn held(&self) -> usize {
        self.slots.len()
    }

    /// Acts on one request at `now`.
    pub fn handle(&mut self, now: Millis, request: Request) -> Handled {
        match request {
            Request::Create(create) => self.create(now, &create),
            Request::Events { device, events } => self.events(now, device, &events),
            Request::Destroy { device } => {
                self.slots.retain(|slot| slot.id != device);
                Ok(None)
            }
        }
    }

    fn create(&mut self, now: Millis, create: &Create) -> Handled {
        let kind = Some(create.kind());
        if let Err(why) = self.admission.admit(create.kind()) {
            return Err(Refused::new(Reason::NotAllowed, kind, why));
        }
        let delivery = create.kind().delivery();
        let over = self.created >= MAX_CREATES_PER_CONNECTION
            || match delivery {
                Delivery::Handed => self.handed >= MAX_HANDED_PER_CONNECTION,
                _ => self.slots.len() >= MAX_DEVICES_PER_CONNECTION,
            };
        if over {
            return Err(Refused::new(
                Reason::TooMany,
                kind,
                "too many devices on one connection",
            ));
        }
        let planned = match plan(create) {
            Ok(planned) => planned,
            Err(why) => return Err(Refused::new(Reason::BadCaps, kind, why)),
        };
        let device = match self.backend.create(&planned.spec) {
            Ok(device) => device,
            Err(why) => return Err(Refused::new(Reason::Unavailable, kind, why)),
        };
        // `created` is at most 64, so the id never wraps and is never reused on a connection.
        let id = DeviceId(self.created);
        self.created += 1;
        match planned.route {
            Route::Proxied(filter) => {
                self.slots.push(Slot {
                    id,
                    device,
                    filter,
                    limiter: Limiter::new(now),
                });
                Ok(Some(Outgoing {
                    reply: Reply::Created {
                        device: id,
                        delivery: Delivery::Proxied,
                    },
                    fd: None,
                }))
            }
            Route::Handed => {
                self.handed += 1;
                Ok(Some(Outgoing {
                    reply: Reply::Created {
                        device: id,
                        delivery: Delivery::Handed,
                    },
                    fd: Some(device.into_fd()),
                }))
            }
        }
    }

    fn events(&mut self, now: Millis, device: DeviceId, events: &[Event]) -> Handled {
        let Self { slots, stats, .. } = self;
        let Some(slot) = slots.iter_mut().find(|slot| slot.id == device) else {
            return Err(Refused::new(
                Reason::Unknown,
                None,
                "events for a device that is not there",
            ));
        };
        match slot.limiter.admit(now) {
            Admit::Pass => {}
            Admit::Drop => {
                stats.dropped_datagrams += 1;
                return Ok(None);
            }
            Admit::Cut => {
                return Err(Refused::new(
                    Reason::TooMany,
                    None,
                    "events arrive faster than any device sends them",
                ));
            }
            _ => return Ok(None),
        }
        let filtered = slot.filter.apply(events);
        let denied = u32::try_from(filtered.denied.len()).unwrap_or(u32::MAX);
        stats.denied_events = stats.denied_events.saturating_add(denied);
        if stats.denied_events >= MAX_DENIED_EVENTS {
            return Err(Refused::new(
                Reason::NotAllowed,
                None,
                "too many events outside the ceiling",
            ));
        }
        if filtered.pass.is_empty() {
            return Ok(None);
        }
        match slot.device.write(&filtered.pass) {
            Ok(()) => Ok(None),
            Err(why) => Err(Refused::new(Reason::Unavailable, None, why)),
        }
    }
}

impl<B: Backend, A: Admission> std::fmt::Debug for Session<'_, B, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("held", &self.slots.len())
            .field("created", &self.created)
            .field("handed", &self.handed)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}
