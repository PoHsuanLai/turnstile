//! The turnstile wire: what a client may ask for, what the service answers, and the fixed list of
//! devices it will make with the per-event ceilings on each.
//!
//! This crate is pure. It reads no file, clock or socket and links nothing but `thiserror`; the
//! daemon and the client both build on it, so a ceiling is written down once. A message is one
//! `SOCK_SEQPACKET` datagram: [`codec`] turns it into a [`message::Request`] or a
//! [`message::Reply`] and back, and [`ceiling::plan`] turns a [`message::Create`] into the device
//! the service would make plus the [`ceiling::Filter`] every event of a proxied device passes.
#![forbid(unsafe_code)]

pub mod ceiling;
pub mod codec;
pub mod codes;
pub mod event;
pub mod keytable;
pub mod kind;
pub mod limiter;
pub mod limits;
pub mod message;
pub mod spec;

pub use ceiling::{BadCaps, Denial, Filter, Filtered, Plan, Verdict, plan};
pub use codec::{WireError, decode_reply, decode_request, encode_reply, encode_request};
pub use event::Event;
pub use kind::{Delivery, Kind, Owner};
pub use limiter::{Admit, Limiter, Millis};
pub use message::{Create, Detail, DeviceId, PointerCaps, Reason, Reply, Request};
pub use spec::{AbsAxis, DeviceSpec};
