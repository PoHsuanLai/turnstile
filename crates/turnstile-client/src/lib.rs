//! What palmrest and keycap link to ask the input-device service for a device.
//!
//! Sync and small: one `SOCK_SEQPACKET` connection, no bus, no async runtime. [`DeviceSource`] is the
//! seam: [`Broker`] talks to the real service, `fake::FakeSource` (feature `fake`) is the double
//! the callers' tests use.
//!
//! ```ignore
//! let source = Broker::default();
//! let mut session = source.open()?;
//! let pad = session.create(&Create::touchpad())?;      // Made::Proxied(id)
//! session.send(id, &frame)?;                           // Sent::Queued, or Sent::Dropped when the service is behind
//! let keys = session.create(&Create::keyboard())?;     // Made::Handed { device, fd }
//! ```
#![forbid(unsafe_code)]

mod broker;
mod error;
#[cfg(feature = "fake")]
pub mod fake;
mod link;
mod session;

pub use broker::{Broker, Connection, DEFAULT_SOCKET};
pub use error::ClientError;
pub use session::{DeviceSource, Made, Sent, Session};
pub use turnstile_wire::{AbsAxis, Create, DeviceId, Event, Kind, PointerCaps, Reason};
