//! One connection from accept to close: identify the peer, then read datagrams and feed them to a
//! [`Session`] until the peer leaves, is refused, or says something that does not parse.

use crate::backend::Backend;
use crate::clock::Clock;
use crate::identity::{Authority, Caller, SeatSource};
use crate::journal::{self, kv};
use crate::link::{self, Inbox, Incoming};
use crate::policy::CallerAdmission;
use crate::session::{Handled, Outgoing, Phase, Session, Step};
use std::os::fd::OwnedFd;
use std::time::Duration;
use turnstile_wire::limits::HANDSHAKE_TIMEOUT_MS;
use turnstile_wire::{Reason, Reply, decode_request};

/// Everything a connection needs from outside, as seams.
#[derive(Debug)]
pub struct Context<B, A, S, C> {
    /// Makes devices.
    pub backend: B,
    /// Names callers.
    pub authority: A,
    /// Says who is logged in.
    pub seat: S,
    /// Tells the time.
    pub clock: C,
}

/// Whether to go on after a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    On,
    Stop,
}

/// Serves `conn` until it ends. Never panics on what the peer sends.
pub fn serve<B, A, S, C>(conn: OwnedFd, ctx: &Context<B, A, S, C>)
where
    B: Backend,
    A: Authority,
    S: SeatSource,
    C: Clock,
{
    let caller = match ctx.authority.identify(&conn) {
        Ok(caller) => caller,
        Err(why) => {
            journal::note("refused", &[kv("why", &why)]);
            let _ = link::send_reply(&conn, &Reply::Refused(Reason::Unknown));
            return;
        }
    };
    let admission = CallerAdmission {
        caller: &caller,
        seat: &ctx.seat,
    };
    let mut session = Session::new(&ctx.backend, &admission);
    let mut inbox = Inbox::new();
    let mut waiting = None;
    loop {
        let want = match session.phase() {
            Phase::Handshake => Some(Duration::from_millis(HANDSHAKE_TIMEOUT_MS)),
            _ => None,
        };
        if want != waiting {
            if link::set_receive_timeout(&conn, want).is_err() {
                return;
            }
            waiting = want;
        }
        let request = match inbox.receive(&conn) {
            Incoming::Datagram(len) => decode_request(inbox.datagram(len)),
            Incoming::Closed => break,
            Incoming::Oversize | Incoming::TimedOut => {
                let _ = link::send_reply(&conn, &Reply::Refused(Reason::Unknown));
                break;
            }
        };
        let handled = match request {
            Ok(request) => session.handle(ctx.clock.now(), request),
            Err(why) => {
                journal::note(
                    "refused",
                    &[kv("caller", &caller.describe()), kv("why", &why)],
                );
                let _ = link::send_reply(&conn, &Reply::Refused(Reason::Unknown));
                break;
            }
        };
        if deliver(&conn, &caller, handled) == Flow::Stop {
            break;
        }
    }
    let stats = session.stats();
    if stats.dropped_datagrams > 0 || stats.denied_events > 0 {
        journal::note(
            "closed",
            &[
                kv("caller", &caller.describe()),
                kv("dropped", &stats.dropped_datagrams),
                kv("denied", &stats.denied_events),
            ],
        );
    }
}

/// Logs a refusal and sends the replies; stops when the session says so or a reply cannot go.
fn deliver(conn: &OwnedFd, caller: &Caller, handled: Handled) -> Flow {
    if let Some(note) = &handled.note {
        let kind = note.kind.map_or("?", |k| k.name());
        journal::note(
            "refused",
            &[
                kv("caller", &caller.describe()),
                kv("kind", &kind),
                kv("why", &note.why),
            ],
        );
    }
    for out in handled.out {
        let sent = match &out {
            Outgoing::Reply(reply) => link::send_reply(conn, reply),
            Outgoing::Handed { reply, fd } => link::send_reply_with_fd(conn, reply, fd),
        };
        if sent.is_err() {
            return Flow::Stop;
        }
    }
    match handled.step {
        Step::Continue => Flow::On,
        _ => Flow::Stop,
    }
}
