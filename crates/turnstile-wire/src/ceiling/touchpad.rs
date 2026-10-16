//! palmrest's touchpad: exactly the capabilities of its `touchpad_caps`, with the fixed ranges.

use super::Plan;
use super::filter::{Denial, Filter, Verdict};
use crate::codes::{
    ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_SLOT, ABS_MT_TRACKING_ID, ABS_X, ABS_Y, BTN_LEFT,
    BTN_TOOL_DOUBLETAP, BTN_TOOL_FINGER, BTN_TOOL_QUADTAP, BTN_TOOL_TRIPLETAP, BTN_TOUCH, EV_ABS,
    EV_KEY, EV_SYN, INPUT_PROP_POINTER, SYN_REPORT,
};
use crate::event::Event;
use crate::spec::{AbsAxis, DeviceSpec};

/// Horizontal extent in device units (palmrest's `PAD_X_MAX`).
pub(super) const X_MAX: i32 = 4000;
/// Vertical extent in device units (palmrest's `PAD_Y_MAX`).
pub(super) const Y_MAX: i32 = 2800;
/// Units per millimetre (palmrest's `UNITS_PER_MM`).
const UNITS_PER_MM: i32 = 40;
/// Contact slots (palmrest's `PAD_SLOTS`).
pub(super) const SLOTS: usize = 5;
/// The largest tracking id palmrest registers.
const TRACKING_MAX: i32 = 0xffff;

/// The buttons and tools the touchpad has.
const KEYS: [u16; 6] = [
    BTN_LEFT,
    BTN_TOUCH,
    BTN_TOOL_FINGER,
    BTN_TOOL_DOUBLETAP,
    BTN_TOOL_TRIPLETAP,
    BTN_TOOL_QUADTAP,
];

pub(super) fn plan() -> Plan {
    let x = AbsAxis::range(ABS_X, 0, X_MAX).with_resolution(UNITS_PER_MM);
    let y = AbsAxis::range(ABS_Y, 0, Y_MAX).with_resolution(UNITS_PER_MM);
    Plan {
        spec: DeviceSpec {
            name: "palmrest touchpad".to_owned(),
            vendor: 0x0001,
            product: 0x0002,
            keys: KEYS.to_vec(),
            rels: Vec::new(),
            abs: vec![
                x,
                y,
                AbsAxis::range(ABS_MT_SLOT, 0, SLOTS as i32 - 1),
                AbsAxis::range(ABS_MT_TRACKING_ID, 0, TRACKING_MAX),
                AbsAxis {
                    code: ABS_MT_POSITION_X,
                    ..x
                },
                AbsAxis {
                    code: ABS_MT_POSITION_Y,
                    ..y
                },
            ],
            props: vec![INPUT_PROP_POINTER],
            leds: Vec::new(),
        },
        filter: Some(Filter::Touchpad(TouchpadState::default())),
    }
}

/// The contacts the pad believes are down and the slot the next events belong to.
#[derive(Debug, Clone, Copy, Default)]
pub struct TouchpadState {
    slot: usize,
    down: [bool; SLOTS],
}

impl TouchpadState {
    fn any_down(&self) -> bool {
        self.down.iter().any(|&d| d)
    }

    pub(super) fn check(&mut self, event: Event) -> Verdict {
        let value = event.value;
        match (event.kind, event.code) {
            (EV_SYN, SYN_REPORT) if value == 0 => Verdict::Pass(event),
            (EV_KEY, code) if KEYS.contains(&code) => self.key(event),
            (EV_ABS, ABS_X | ABS_MT_POSITION_X) => Verdict::Pass(clamp(event, X_MAX)),
            (EV_ABS, ABS_Y | ABS_MT_POSITION_Y) => Verdict::Pass(clamp(event, Y_MAX)),
            (EV_ABS, ABS_MT_SLOT) => match usize::try_from(value) {
                Ok(slot) if slot < SLOTS => {
                    self.slot = slot;
                    Verdict::Pass(event)
                }
                _ => Verdict::Deny(Denial::OutOfRange),
            },
            (EV_ABS, ABS_MT_TRACKING_ID) if (-1..=TRACKING_MAX).contains(&value) => {
                if let Some(down) = self.down.get_mut(self.slot) {
                    *down = value >= 0;
                }
                Verdict::Pass(event)
            }
            (EV_ABS, ABS_MT_TRACKING_ID) => Verdict::Deny(Denial::OutOfRange),
            _ => Verdict::Deny(Denial::NotAllowed),
        }
    }

    /// A button or tool changes only to 0 or 1; a click needs a finger down.
    fn key(&self, event: Event) -> Verdict {
        match (event.code, event.value) {
            (BTN_LEFT, 1) if !self.any_down() => Verdict::Deny(Denial::NoContact),
            (_, 0 | 1) => Verdict::Pass(event),
            (_, _) => Verdict::Deny(Denial::BadValue),
        }
    }
}

fn clamp(event: Event, max: i32) -> Event {
    Event {
        value: event.value.clamp(0, max),
        ..event
    }
}
