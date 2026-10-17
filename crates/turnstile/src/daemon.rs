//! The accept loop: take connections from the socket systemd passed in, one thread each, up to a
//! cap, and leave when no one has been connected for a while.

use crate::backend::Backend;
use crate::clock::Clock;
use crate::identity::{Authority, SeatSource};
use crate::idle::{Idle, Idling};
use crate::journal::{self, kv};
use crate::link;
use crate::serve::{Context, identify, serve_caller};
use crate::slots::{Bucket, Slots};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::net::{SocketFlags, accept_with};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::Duration;
use turnstile_wire::{Reason, Reply};

/// How long the service waits with no one connected before it exits: socket activation starts it
/// again on the next connection, so an idle minute costs nothing.
pub const IDLE_EXIT: Duration = Duration::from_secs(60);
/// How often the loop looks at the clock when no connection arrives.
pub const POLL_SLICE: Duration = Duration::from_secs(1);
/// The stack of a connection thread: it holds a 2 KiB buffer and a few small vectors.
const THREAD_STACK_BYTES: usize = 256 * 1024;
/// How long to wait after the system refused a descriptor or memory, so a full table is not a spin.
const BACKOFF: Duration = Duration::from_millis(100);

/// Timing of the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Exit after this long with no connection.
    pub idle: Duration,
    /// Look at the clock at least this often.
    pub slice: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle: IDLE_EXIT,
            slice: POLL_SLICE,
        }
    }
}

/// Why the loop failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DaemonError {
    /// Waiting on the listening socket failed.
    #[error("waiting for connections failed: {0}")]
    Poll(std::io::Error),
    /// Accepting failed in a way that will not pass.
    #[error("accepting a connection failed: {0}")]
    Accept(std::io::Error),
}

/// The service.
#[derive(Debug)]
pub struct Daemon<B, A, S, C> {
    listener: OwnedFd,
    context: Arc<Context<B, A, S, C>>,
    settings: Settings,
}

impl<B, A, S, C> Daemon<B, A, S, C>
where
    B: Backend,
    A: Authority,
    S: SeatSource,
    C: Clock,
{
    /// A service on `listener`, a listening `SOCK_SEQPACKET` socket.
    pub fn new(listener: OwnedFd, context: Context<B, A, S, C>, settings: Settings) -> Self {
        Self {
            listener,
            context: Arc::new(context),
            settings,
        }
    }

    /// Serves until idle for [`Settings::idle`], then returns.
    pub fn run(&self) -> Result<(), DaemonError> {
        let slots = Slots::new();
        let limit_ms = u64::try_from(self.settings.idle.as_millis()).unwrap_or(u64::MAX);
        let mut idle = Idle::new(limit_ms);
        loop {
            if idle.observe(self.context.clock.now(), slots.open()) == Idling::Exit {
                return Ok(());
            }
            if self.waiting_connection()? {
                self.accept_one(&slots)?;
            }
        }
    }

    /// Whether a connection is waiting, after at most one slice.
    fn waiting_connection(&self) -> Result<bool, DaemonError> {
        let slice = Timespec {
            tv_sec: i64::try_from(self.settings.slice.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: i64::from(self.settings.slice.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&self.listener, PollFlags::IN)];
        match poll(&mut fds, Some(&slice)) {
            Ok(ready) => Ok(ready > 0),
            Err(Errno::INTR) => Ok(false),
            Err(e) => Err(DaemonError::Poll(e.into())),
        }
    }

    /// Takes one waiting connection: names its peer first, then gives it a slot of its own pool, so
    /// that a flood from one user can fill that user's slots and nobody else's (NEW-PI-6).
    fn accept_one(&self, slots: &Slots) -> Result<(), DaemonError> {
        let conn = match accept_with(&self.listener, SocketFlags::CLOEXEC) {
            Ok(conn) => conn,
            Err(Errno::AGAIN | Errno::INTR | Errno::CONNABORTED) => return Ok(()),
            Err(e @ (Errno::MFILE | Errno::NFILE | Errno::NOBUFS | Errno::NOMEM)) => {
                journal::note("accept", &[kv("why", &e)]);
                std::thread::sleep(BACKOFF);
                return Ok(());
            }
            Err(e) => return Err(DaemonError::Accept(e.into())),
        };
        let Some(caller) = identify(&conn, &self.context.authority) else {
            return Ok(());
        };
        let bucket = Bucket::of(&caller, self.context.seat.active_uid());
        let slot = match slots.take(bucket) {
            Ok(slot) => slot,
            Err(_) => {
                journal::note(
                    "refused",
                    &[
                        kv("caller", &caller.describe()),
                        kv("why", &"too many connections"),
                    ],
                );
                let _ = link::send_reply(&conn, &Reply::Refused(Reason::TooMany));
                return Ok(());
            }
        };
        let context = Arc::clone(&self.context);
        let spawned = std::thread::Builder::new()
            .name("connection".to_owned())
            .stack_size(THREAD_STACK_BYTES)
            .spawn(move || {
                let slot = slot;
                serve_caller(conn, caller, slot.bucket().handshake_ms(), &context);
            });
        if let Err(why) = spawned {
            journal::note("spawn", &[kv("why", &why)]);
        }
        Ok(())
    }
}
