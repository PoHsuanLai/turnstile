//! palmrest's wheel: four relative axes and nothing else.

use super::Plan;
use super::filter::{Denial, Filter, Verdict};
use crate::codes::{
    EV_REL, EV_SYN, REL_HWHEEL, REL_HWHEEL_HI_RES, REL_WHEEL, REL_WHEEL_HI_RES, SYN_REPORT,
};
use crate::event::Event;
use crate::limits::MAX_WHEEL_MAGNITUDE;
use crate::maker::Maker;
use crate::spec::DeviceSpec;

/// The relative axes the wheel has.
pub(super) const AXES: [u16; 4] = [REL_WHEEL, REL_HWHEEL, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES];

pub(super) fn plan(maker: Maker) -> Plan {
    Plan {
        spec: DeviceSpec {
            name: "palmrest wheel".to_owned(),
            maker,
            vendor: 0x0001,
            product: 0x0001,
            keys: Vec::new(),
            rels: AXES.to_vec(),
            abs: Vec::new(),
            props: Vec::new(),
            leds: Vec::new(),
        },
        filter: Some(Filter::Wheel),
    }
}

pub(super) fn check(event: Event) -> Verdict {
    match (event.kind, event.code) {
        (EV_SYN, SYN_REPORT) if event.value == 0 => Verdict::Pass(event),
        (EV_REL, code) if AXES.contains(&code) => {
            if event.value.unsigned_abs() <= MAX_WHEEL_MAGNITUDE.unsigned_abs() {
                Verdict::Pass(event)
            } else {
                Verdict::Deny(Denial::OutOfRange)
            }
        }
        _ => Verdict::Deny(Denial::NotAllowed),
    }
}
