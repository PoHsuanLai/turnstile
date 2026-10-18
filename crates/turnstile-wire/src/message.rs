//! The messages as values. [`crate::codec`] turns them into datagrams and back.

use crate::event::Event;
use crate::kind::{Delivery, Kind};
use crate::spec::AbsAxis;

/// A device the service made for this connection; one byte, never reused on a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u8);

/// What a keycap pointer asks for: the pointer half of a combo device. The service cuts it to the
/// pointer ceiling, so a pointer can never register a key below `BTN_MISC`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PointerCaps {
    /// The original device's name; the service adds the fixed suffix `(keycap pointer)`.
    pub label: String,
    /// The original's vendor id, so per-device quirks still match.
    pub vendor: u16,
    /// The original's product id.
    pub product: u16,
    /// `INPUT_PROP_*` bits.
    pub props: Vec<u16>,
    /// `BTN_*` codes.
    pub buttons: Vec<u16>,
    /// Relative axes.
    pub rels: Vec<u16>,
    /// Absolute axes with their ranges.
    pub abs: Vec<AbsAxis>,
}

/// A request for one device. Each kind carries what it needs, so a pair that does not belong
/// together cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Create {
    /// palmrest's touchpad.
    Touchpad,
    /// palmrest's wheel.
    Wheel,
    /// palmrest's keyboard with these key codes.
    Keys(Vec<u16>),
    /// keycap's keyboard.
    Keyboard,
    /// keycap's pointer with these capabilities.
    Pointer(PointerCaps),
}

impl Create {
    /// palmrest's touchpad.
    pub const fn touchpad() -> Self {
        Self::Touchpad
    }

    /// palmrest's wheel.
    pub const fn wheel() -> Self {
        Self::Wheel
    }

    /// palmrest's keyboard with the keys `codes`.
    pub fn keys(codes: Vec<u16>) -> Self {
        Self::Keys(codes)
    }

    /// keycap's keyboard.
    pub const fn keyboard() -> Self {
        Self::Keyboard
    }

    /// keycap's pointer with `caps`.
    pub fn pointer(caps: PointerCaps) -> Self {
        Self::Pointer(caps)
    }

    /// The kind of device asked for.
    pub const fn kind(&self) -> Kind {
        match self {
            Self::Touchpad => Kind::PalmrestTouchpad,
            Self::Wheel => Kind::PalmrestWheel,
            Self::Keys(_) => Kind::PalmrestKeys,
            Self::Keyboard => Kind::KeycapKeyboard,
            Self::Pointer(_) => Kind::KeycapPointer,
        }
    }
}

/// Client to service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Make a device.
    Create(Create),
    /// Write events to a proxied device.
    Events {
        /// The device.
        device: DeviceId,
        /// Up to [`crate::limits::MAX_EVENTS_PER_DATAGRAM`] events.
        events: Vec<Event>,
    },
    /// Destroy a device now; closing the connection destroys every device it made.
    Destroy {
        /// The device.
        device: DeviceId,
    },
}

/// Why the service refused. After it the service closes the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Reason {
    /// A message or device the service does not know.
    Unknown,
    /// The caller may not have this.
    NotAllowed,
    /// Over a cap.
    TooMany,
    /// The capabilities are outside the ceiling.
    BadCaps,
    /// The kernel would not make the device.
    Unavailable,
}

impl Reason {
    /// The byte on the wire.
    pub const fn code(self) -> u8 {
        match self {
            Self::Unknown => 1,
            Self::NotAllowed => 2,
            Self::TooMany => 3,
            Self::BadCaps => 4,
            Self::Unavailable => 5,
        }
    }

    /// The reason with wire byte `code`; a code from a newer service reads as `Unknown`.
    pub const fn from_code(code: u8) -> Self {
        match code {
            2 => Self::NotAllowed,
            3 => Self::TooMany,
            4 => Self::BadCaps,
            5 => Self::Unavailable,
            _ => Self::Unknown,
        }
    }
}

/// Service to client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// The device exists. For [`Delivery::Handed`] its file descriptor travels with this datagram.
    Created {
        /// The device.
        device: DeviceId,
        /// How events reach the kernel.
        delivery: Delivery,
    },
    /// Lights, repeat or sound events the desktop wrote to a proxied device. The ceilings give
    /// every proxied kind no output capability, so the service sends none today.
    Feedback {
        /// The device.
        device: DeviceId,
        /// Up to [`crate::limits::MAX_FEEDBACK_EVENTS`] events.
        events: Vec<Event>,
    },
    /// The request was refused and the connection is closing.
    Refused(Reason),
}
