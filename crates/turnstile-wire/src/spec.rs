//! The device the service would make: every number the kernel is told. Built only by
//! [`crate::ceiling::plan`], from constants of the kind and the client's request cut to the ceiling;
//! a client's name, ids or axis ranges never reach the kernel except through a pointer's label.

use crate::codes::{EV_ABS, EV_KEY, EV_LED, EV_REL};
use crate::maker::Maker;

/// One absolute axis with its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsAxis {
    /// `ABS_*`.
    pub code: u16,
    /// The least value.
    pub minimum: i32,
    /// The greatest value.
    pub maximum: i32,
    /// Noise filter.
    pub fuzz: i32,
    /// Dead zone.
    pub flat: i32,
    /// Units per millimetre.
    pub resolution: i32,
}

impl AbsAxis {
    /// An axis from `minimum` to `maximum` with no fuzz, flat or resolution.
    pub const fn range(code: u16, minimum: i32, maximum: i32) -> Self {
        Self {
            code,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution: 0,
        }
    }

    /// The same axis with `resolution` units per millimetre.
    pub const fn with_resolution(self, resolution: i32) -> Self {
        Self { resolution, ..self }
    }
}

/// A device on the virtual bus, as the kernel is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSpec {
    /// The name the desktop sees.
    pub name: String,
    /// The service it is made for, written into the device as its label.
    pub maker: Maker,
    /// Vendor id.
    pub vendor: u16,
    /// Product id.
    pub product: u16,
    /// Key and button codes.
    pub keys: Vec<u16>,
    /// Relative axes.
    pub rels: Vec<u16>,
    /// Absolute axes.
    pub abs: Vec<AbsAxis>,
    /// `INPUT_PROP_*` bits.
    pub props: Vec<u16>,
    /// Keyboard lights, which the desktop writes back to the device.
    pub leds: Vec<u16>,
}

impl DeviceSpec {
    /// The event types besides `EV_SYN` the device declares, from what it has.
    pub fn event_types(&self) -> Vec<u16> {
        [
            (EV_KEY, !self.keys.is_empty()),
            (EV_REL, !self.rels.is_empty()),
            (EV_ABS, !self.abs.is_empty()),
            (EV_LED, !self.leds.is_empty()),
        ]
        .into_iter()
        .filter_map(|(kind, present)| present.then_some(kind))
        .collect()
    }
}
