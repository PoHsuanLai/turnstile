//! The accept loop: take connections from the socket systemd passed in, one thread each, up to a
//! cap, and leave when no one has been connected for a while.
//!
//! The loop itself reads nothing of a peer but the user the kernel names from the socket (NEW-FA-3):
//! naming the rest can wait for ever on a program the peer controls, so it happens on the
//! connection's own thread ([`crate::admit`]), and the loop only counts how many such threads each
//! user has waiting ([`crate::pending`]) and closes a connection over its user's share at once.
//! Slots are still counted after identification, so the reserve of palmrest and keycap is safe.

use crate::admit::admit;
use crate::backend::Backend;
use crate::clock::Clock;
use crate::identity::{Authority, SeatSource};
use crate::idle::{Idle, Idling};
use crate::journal::{self, kv};
use crate::link;
use crate::pending::{Caps, Pending};
use crate::serve::Context;
use crate::slots::Slots;
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
/// The stack of a connection thread: identification reads and parses the table and the peer's
/// `/proc` entries before the 2 KiB buffer and few small vectors of serving. Only touched pages
/// count, so a megabyte costs little.
const THREAD_STACK_BYTES: usize = 1024 * 1024;
/// How long to wait after the system refused a descriptor or memory, so a full table is not a spin.
const BACKOFF: Duration = Duration::from_millis(100);

/// Timing of the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Exit after this long with no connection.
    pub idle: Duration,
    /// Look at the clock at least this often.
    pub slice: Duration,
    /// How many connections may be waiting to be identified.
    pub pending: Caps,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle: IDLE_EXIT,
            slice: POLL_SLICE,
            pending: Caps::default(),
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
        let pending = Pending::new(self.settings.pending);
        let limit_ms = u64::try_from(self.settings.idle.as_millis()).unwrap_or(u64::MAX);
        let mut idle = Idle::new(limit_ms);
        loop {
            let busy = slots.open() + pending.count();
            if idle.observe(self.context.clock.now(), busy) == Idling::Exit {
                return Ok(());
            }
            if self.waiting_connection()? {
                self.accept_one(&slots, &pending)?;
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

    /// Takes one waiting connection and hands it to a thread of its own, unless its user already
    /// has as many waiting to be identified as it may (NEW-FA-3).
    fn accept_one(&self, slots: &Slots, pending: &Pending) -> Result<(), DaemonError> {
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
        let uid = match self.context.authority.peer_uid(&conn) {
            Ok(uid) => uid,
            Err(why) => {
                journal::note("refused", &[kv("why", &why)]);
                let _ = link::send_reply(&conn, &Reply::Refused(Reason::Unknown));
                return Ok(());
            }
        };
        let ticket = match pending.enter(uid) {
            Ok(ticket) => ticket,
            Err(why) => {
                journal::note("refused", &[kv("uid", &uid), kv("why", &why)]);
                let _ = link::send_reply(&conn, &Reply::Refused(Reason::TooMany));
                return Ok(());
            }
        };
        let context = Arc::clone(&self.context);
        let slots = slots.clone();
        let spawned = std::thread::Builder::new()
            .name("connection".to_owned())
            .stack_size(THREAD_STACK_BYTES)
            .spawn(move || admit(conn, ticket, &slots, &context));
        if let Err(why) = spawned {
            journal::note("spawn", &[kv("why", &why)]);
        }
        Ok(())
    }
}
