//! The allow-list: for each kind, the device the service makes and the ceiling every event of a
//! proxied device passes. Palmrest's three kinds are proxied event by event; keycap's two are made
//! to a fixed shape and handed over, because a uinput descriptor cannot be made write-only (the
//! holder can destroy the device and set it up again with any capabilities), so for those the
//! caller's identity is the gate and the ceiling only shapes the device the service itself makes.

mod filter;
mod keycap;
mod keys;
mod touchpad;
mod wheel;

pub use filter::{Denial, Filter, Filtered, Verdict};
pub use keycap::{KEYBOARD_NAME, POINTER_SUFFIX};

use crate::kind::Kind;
use crate::message::{Create, Detail};
use crate::spec::DeviceSpec;

/// What the service does for one `Create`.
#[derive(Debug)]
pub struct Plan {
    /// The device to make.
    pub spec: DeviceSpec,
    /// The ceiling for its events; `None` for a handed device, whose events the service never sees.
    pub filter: Option<Filter>,
}

/// Why a request is outside the ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BadCaps {
    /// The description does not belong to the kind.
    #[error("the description does not fit the device kind")]
    Detail,
    /// A keyboard with no keys.
    #[error("the key list is empty")]
    NoKeys,
    /// A key that palmrest's keyboard may not have.
    #[error("key {0} is not one the keyboard may have")]
    UnknownKey(u16),
    /// A pointer with nothing a pointer can do after the cut.
    #[error("the pointer has no buttons, relative axes or absolute axes within the ceiling")]
    EmptyPointer,
    /// An absolute axis with a code or a range outside the ceiling.
    #[error("absolute axis {0} is outside the ceiling")]
    BadAxis(u16),
}

/// Cuts `create` to the ceiling of its kind.
pub fn plan(create: &Create) -> Result<Plan, BadCaps> {
    let maker = create.kind.maker();
    match (create.kind, &create.detail) {
        (Kind::PalmrestTouchpad, Detail::None) => Ok(touchpad::plan(maker)),
        (Kind::PalmrestWheel, Detail::None) => Ok(wheel::plan(maker)),
        (Kind::PalmrestKeys, Detail::Keys(codes)) => keys::plan(maker, codes),
        (Kind::KeycapKeyboard, Detail::None) => Ok(keycap::keyboard(maker)),
        (Kind::KeycapPointer, Detail::Pointer(caps)) => keycap::pointer(maker, caps),
        (_, _) => Err(BadCaps::Detail),
    }
}
