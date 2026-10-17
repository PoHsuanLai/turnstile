//! One new connection on its own thread: identify the peer, count it into its pool of slots, then
//! serve it (NEW-FA-3). Identification may wait on the peer, so it never runs on the accept loop;
//! the ticket in [`crate::pending`] holds the connection's place until the slot is decided, so
//! identification still comes before counting (NEW-PI-6).

use crate::backend::Backend;
use crate::clock::Clock;
use crate::identity::{Authority, SeatSource};
use crate::journal::{self, kv};
use crate::link;
use crate::pending::Ticket;
use crate::serve::{Context, identify, serve_caller};
use crate::slots::{Bucket, Slots};
use std::os::fd::OwnedFd;
use turnstile_wire::{Reason, Reply};

/// Identifies `conn`, gives it a slot of its pool, and serves it until it ends. A caller whose pool
/// is full is answered `TooMany`.
pub fn admit<B, A, S, C>(conn: OwnedFd, ticket: Ticket, slots: &Slots, ctx: &Context<B, A, S, C>)
where
    B: Backend,
    A: Authority,
    S: SeatSource,
    C: Clock,
{
    let Some(caller) = identify(&conn, &ctx.authority) else {
        return;
    };
    let bucket = Bucket::of(&caller, ctx.seat.active_uid());
    let taken = slots.take(bucket);
    drop(ticket);
    match taken {
        Ok(slot) => serve_caller(conn, caller, slot.bucket().handshake_ms(), ctx),
        Err(_) => {
            journal::note(
                "refused",
                &[
                    kv("caller", &caller.describe()),
                    kv("why", &"too many connections"),
                ],
            );
            let _ = link::send_reply(&conn, &Reply::Refused(Reason::TooMany));
        }
    }
}
