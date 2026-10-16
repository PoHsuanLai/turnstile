//! The per-event ceiling of a proxied device.

use super::keys::KeysState;
use super::touchpad::TouchpadState;
use super::wheel;
use crate::event::Event;

/// Why an event was not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Denial {
    /// The type or code is not one the device has.
    NotAllowed,
    /// The value is outside what the device reports.
    OutOfRange,
    /// A value the type does not take (a key repeat, a key value of 7).
    BadValue,
    /// A letter, digit, Enter, Space, Backspace, Delete or Escape with no control, alt or super held.
    NeedsChord,
    /// A click while no finger is down.
    NoContact,
    /// More keys down at once than a combination needs.
    TooManyKeys,
}

/// What the ceiling says about one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// Write this event (a position outside the range is clamped into it).
    Pass(Event),
    /// Do not write it.
    Deny(Denial),
}

/// The events that passed and the reasons the others did not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filtered {
    /// In order.
    pub pass: Vec<Event>,
    /// One per denied event.
    pub denied: Vec<Denial>,
}

/// The ceiling and the little state it tracks (fingers down, keys down).
#[derive(Debug)]
pub enum Filter {
    /// palmrest's touchpad.
    Touchpad(TouchpadState),
    /// palmrest's wheel.
    Wheel,
    /// palmrest's keyboard.
    Keys(KeysState),
}

impl Filter {
    /// Judges one event and updates the state it tracks.
    pub fn check(&mut self, event: Event) -> Verdict {
        match self {
            Self::Touchpad(state) => state.check(event),
            Self::Wheel => wheel::check(event),
            Self::Keys(state) => state.check(event),
        }
    }

    /// Judges `events` in order.
    pub fn apply(&mut self, events: &[Event]) -> Filtered {
        events.iter().fold(Filtered::default(), |mut out, &event| {
            match self.check(event) {
                Verdict::Pass(e) => out.pass.push(e),
                Verdict::Deny(why) => out.denied.push(why),
            }
            out
        })
    }
}
