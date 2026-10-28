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

pub use filter::{Denial, Filter, Filtered};
pub use keycap::{KEYBOARD_NAME, POINTER_SUFFIX};

use crate::message::Create;
use crate::spec::DeviceSpec;

/// How events of a planned device reach the kernel.
#[derive(Debug)]
pub enum Route {
    /// The service writes each event after the ceiling `Filter` has passed it.
    Proxied(Filter),
    /// The device is handed over, and its events are never the service's to see.
    Handed,
}

/// What the service does for one `Create`.
#[derive(Debug)]
pub struct Plan {
    /// The device to make.
    pub spec: DeviceSpec,
    /// How its events reach the kernel.
    pub route: Route,
}

/// Why a request is outside the ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BadCaps {
    /// A keyboard with more keys than a request may list.
    #[error("the key list is longer than a request may make it")]
    TooManyKeys,
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
    let maker = create.kind().maker();
    match create {
        Create::Touchpad => Ok(touchpad::plan(maker)),
        Create::Wheel => Ok(wheel::plan(maker)),
        Create::Keys(codes) => keys::plan(maker, codes),
        Create::Keyboard => Ok(keycap::keyboard(maker)),
        Create::Pointer(caps) => keycap::pointer(maker, caps),
    }
}
