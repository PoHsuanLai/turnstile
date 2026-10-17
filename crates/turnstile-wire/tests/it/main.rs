//! The wire crate's tests: one binary, tables over one-off cases, no I/O.

mod ceiling_keycap;
mod ceiling_keys;
mod ceiling_pad;
mod codec;
mod kinds;
mod limiter;
mod maker;
mod palmrest_table;

use turnstile_wire::Event;
use turnstile_wire::codes::{EV_ABS, EV_KEY, EV_REL, EV_SYN};

pub(crate) fn key(code: u16, value: i32) -> Event {
    Event::new(EV_KEY, code, value)
}

pub(crate) fn abs(code: u16, value: i32) -> Event {
    Event::new(EV_ABS, code, value)
}

pub(crate) fn rel(code: u16, value: i32) -> Event {
    Event::new(EV_REL, code, value)
}

pub(crate) fn sync() -> Event {
    Event::new(EV_SYN, 0, 0)
}
