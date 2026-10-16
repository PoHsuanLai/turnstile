//! keycap's two devices. Both are handed over, so these are the shapes the service itself creates.
//!
//! The keyboard is the full keyboard of keycap's `uinput.rs`: every code from 1 to `KEY_MAX`, the
//! six pointer axes a combo device passes through, and the three lights. The pointer is the twin
//! of one combo device: its request is cut to buttons, relative axes, absolute axes with sane
//! ranges and pointer properties, so the device the service makes can never register a key below
//! `BTN_MISC`. A pointer whose request has nothing left after the cut is refused.

use super::{BadCaps, Plan};
use crate::codes::{
    ABS_MAX, KEY_MAX, LED_CAPSL, LED_NUML, LED_SCROLLL, REL_HWHEEL, REL_HWHEEL_HI_RES, REL_MAX,
    REL_WHEEL, REL_WHEEL_HI_RES, REL_X, REL_Y,
};
use crate::limits::MAX_LABEL_BYTES;
use crate::message::PointerCaps;
use crate::spec::{AbsAxis, DeviceSpec};

/// What follows the original's name in a pointer's name. keycap never grabs a device that ends with it.
pub const POINTER_SUFFIX: &str = "(keycap pointer)";
/// The keyboard's name; keycap never grabs a device with it.
pub const KEYBOARD_NAME: &str = "keycap keyboard";
/// "KC" in ASCII: not a registered vendor, only different from palmrest's and every real keyboard's.
const VENDOR: u16 = 0x4b43;
/// The highest property bit a pointer may have (`INPUT_PROP_POINTING_STICK`).
const PROP_MAX: u16 = 5;
/// The magnitude an axis bound may have: far above any real pad, far below overflow.
const AXIS_LIMIT: i32 = 1 << 24;

/// Whether `code` is a button (`BTN_MISC` to `BTN_GEAR_UP`, or the trigger-happy block) and not a key.
const fn is_button(code: u16) -> bool {
    matches!(code, 0x100..=0x15f | 0x2c0..=0x2ff)
}

pub(super) fn keyboard() -> Plan {
    Plan {
        spec: DeviceSpec {
            name: KEYBOARD_NAME.to_owned(),
            vendor: VENDOR,
            product: 0x0001,
            keys: (1..=KEY_MAX).collect(),
            rels: vec![
                REL_X,
                REL_Y,
                REL_WHEEL,
                REL_HWHEEL,
                REL_WHEEL_HI_RES,
                REL_HWHEEL_HI_RES,
            ],
            abs: Vec::new(),
            props: Vec::new(),
            leds: vec![LED_NUML, LED_CAPSL, LED_SCROLLL],
        },
        filter: None,
    }
}

pub(super) fn pointer(caps: &PointerCaps) -> Result<Plan, BadCaps> {
    let buttons = sorted(caps.buttons.iter().copied().filter(|&c| is_button(c)));
    let rels = sorted(caps.rels.iter().copied().filter(|&c| c <= REL_MAX));
    let props = sorted(caps.props.iter().copied().filter(|&p| p <= PROP_MAX));
    let abs = axes(&caps.abs)?;
    if buttons.is_empty() && rels.is_empty() && abs.is_empty() {
        return Err(BadCaps::EmptyPointer);
    }
    Ok(Plan {
        spec: DeviceSpec {
            name: pointer_name(&caps.label),
            vendor: caps.vendor,
            product: caps.product,
            keys: buttons,
            rels,
            abs,
            props,
            leds: Vec::new(),
        },
        filter: None,
    })
}

/// The label cut to fit, then the fixed suffix.
fn pointer_name(label: &str) -> String {
    let mut cut = label.len().min(MAX_LABEL_BYTES);
    while !label.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{} {POINTER_SUFFIX}", &label[..cut])
}

fn sorted(codes: impl Iterator<Item = u16>) -> Vec<u16> {
    let mut v: Vec<u16> = codes.collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Axes with a known code and a sane range, one per code; anything else refuses the request.
fn axes(requested: &[AbsAxis]) -> Result<Vec<AbsAxis>, BadCaps> {
    let mut out: Vec<AbsAxis> = Vec::with_capacity(requested.len());
    for axis in requested {
        let sane = axis.code <= ABS_MAX
            && axis.minimum <= axis.maximum
            && [axis.minimum, axis.maximum, axis.fuzz, axis.flat]
                .iter()
                .all(|v| v.unsigned_abs() <= AXIS_LIMIT.unsigned_abs())
            && (0..=AXIS_LIMIT).contains(&axis.resolution);
        if !sane {
            return Err(BadCaps::BadAxis(axis.code));
        }
        if out.iter().all(|a| a.code != axis.code) {
            out.push(*axis);
        }
    }
    out.sort_unstable_by_key(|a| a.code);
    Ok(out)
}
