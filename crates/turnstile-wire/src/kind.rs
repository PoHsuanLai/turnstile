//! The five kinds of device turnstile will make, and nothing else. Everything about a kind that is
//! a fact (its code, who may ask, how events reach the kernel) lives here.

use crate::maker::Maker;

/// How events reach the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Delivery {
    /// The client sends events, the service checks each one against the ceiling and writes it.
    Proxied,
    /// The service makes the device and hands the client its file descriptor.
    Handed,
}

impl Delivery {
    /// The byte on the wire.
    pub const fn code(self) -> u8 {
        match self {
            Self::Proxied => 0,
            Self::Handed => 1,
        }
    }

    /// The delivery with wire byte `code`.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Proxied),
            1 => Some(Self::Handed),
            _ => None,
        }
    }
}

/// A kind of device the service will make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// palmrest's virtual touchpad.
    PalmrestTouchpad,
    /// palmrest's virtual scroll wheel.
    PalmrestWheel,
    /// palmrest's virtual keyboard, for gesture key combinations only.
    PalmrestKeys,
    /// keycap's virtual keyboard.
    KeycapKeyboard,
    /// keycap's virtual pointer, the pointer half of a combo device.
    KeycapPointer,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Kind; 5] = [
        Kind::PalmrestTouchpad,
        Kind::PalmrestWheel,
        Kind::PalmrestKeys,
        Kind::KeycapKeyboard,
        Kind::KeycapPointer,
    ];

    /// The byte on the wire.
    pub const fn code(self) -> u8 {
        match self {
            Self::PalmrestTouchpad => 1,
            Self::PalmrestWheel => 2,
            Self::PalmrestKeys => 3,
            Self::KeycapKeyboard => 16,
            Self::KeycapPointer => 17,
        }
    }

    /// The kind with wire byte `code`.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::PalmrestTouchpad),
            2 => Some(Self::PalmrestWheel),
            3 => Some(Self::PalmrestKeys),
            16 => Some(Self::KeycapKeyboard),
            17 => Some(Self::KeycapPointer),
            _ => None,
        }
    }

    /// How events of this kind reach the kernel.
    pub const fn delivery(self) -> Delivery {
        match self {
            Self::PalmrestTouchpad | Self::PalmrestWheel | Self::PalmrestKeys => Delivery::Proxied,
            Self::KeycapKeyboard | Self::KeycapPointer => Delivery::Handed,
        }
    }

    /// The only service that may ask for this kind, and the one every device of it is made for.
    pub const fn maker(self) -> Maker {
        match self {
            Self::PalmrestTouchpad | Self::PalmrestWheel | Self::PalmrestKeys => Maker::PadDaemon,
            Self::KeycapKeyboard | Self::KeycapPointer => Maker::KeymapDaemon,
        }
    }

    /// A short name for the log.
    pub const fn name(self) -> &'static str {
        match self {
            Self::PalmrestTouchpad => "palmrest-touchpad",
            Self::PalmrestWheel => "palmrest-wheel",
            Self::PalmrestKeys => "palmrest-keys",
            Self::KeycapKeyboard => "keycap-keyboard",
            Self::KeycapPointer => "keycap-pointer",
        }
    }
}
