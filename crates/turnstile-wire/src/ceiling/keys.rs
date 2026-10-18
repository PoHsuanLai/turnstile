//! palmrest's keyboard: only keys of the name table, never auto-repeat, and no typing. A letter,
//! digit, Enter, Space, Backspace, Delete or Escape goes down only while control, alt or super is
//! held, so a compromised palmrest can press shortcuts but not write text. That is a large step
//! down from arbitrary typing, not a proof of safety (`Ctrl+M` is Enter in a terminal): the gate
//! is the caller's identity.

use super::filter::{Denial, Filter, Verdict};
use super::{BadCaps, Plan, Route};
use crate::codes::{EV_KEY, EV_SYN, KEY_LEFTALT, KEY_LEFTCTRL, KEY_LEFTMETA, SYN_REPORT};
use crate::event::Event;
use crate::keytable::{KeyClass, class_of};
use crate::limits::{MAX_KEY_CODES, MAX_KEYS_DOWN};
use crate::maker::Maker;
use crate::spec::DeviceSpec;

/// The modifiers that turn a key press into a shortcut. Shift alone does not.
const CHORD_MODIFIERS: [u16; 3] = [KEY_LEFTCTRL, KEY_LEFTALT, KEY_LEFTMETA];

pub(super) fn plan(maker: Maker, requested: &[u16]) -> Result<Plan, BadCaps> {
    if requested.is_empty() {
        return Err(BadCaps::NoKeys);
    }
    if requested.len() > MAX_KEY_CODES {
        return Err(BadCaps::TooManyKeys);
    }
    if let Some(&bad) = requested.iter().find(|&&c| class_of(c).is_none()) {
        return Err(BadCaps::UnknownKey(bad));
    }
    let mut keys = requested.to_vec();
    keys.sort_unstable();
    keys.dedup();
    Ok(Plan {
        spec: DeviceSpec {
            name: "palmrest keys".to_owned(),
            maker,
            vendor: 0x0001,
            product: 0x0003,
            keys: keys.clone(),
            rels: Vec::new(),
            abs: Vec::new(),
            props: Vec::new(),
            leds: Vec::new(),
        },
        route: Route::Proxied(Filter::Keys(KeysState {
            allowed: keys,
            down: Vec::new(),
        })),
    })
}

/// The keys the device registered and the keys the service believes are down.
#[derive(Debug, Clone)]
pub struct KeysState {
    allowed: Vec<u16>,
    down: Vec<u16>,
}

impl KeysState {
    fn chord_held(&self) -> bool {
        self.down.iter().any(|c| CHORD_MODIFIERS.contains(c))
    }

    pub(super) fn check(&mut self, event: Event) -> Verdict {
        match (event.kind, event.code) {
            (EV_SYN, SYN_REPORT) if event.value == 0 => Verdict::Pass(event),
            (EV_KEY, code) if self.allowed.contains(&code) => match event.value {
                0 => {
                    self.down.retain(|&d| d != code);
                    Verdict::Pass(event)
                }
                1 => self.press(event),
                _ => Verdict::Deny(Denial::BadValue),
            },
            _ => Verdict::Deny(Denial::NotAllowed),
        }
    }

    fn press(&mut self, event: Event) -> Verdict {
        let code = event.code;
        if self.down.contains(&code) {
            return Verdict::Pass(event);
        }
        let class = class_of(code);
        if class == Some(KeyClass::Chorded) && !self.chord_held() {
            return Verdict::Deny(Denial::NeedsChord);
        }
        if self.down.len() >= MAX_KEYS_DOWN {
            return Verdict::Deny(Denial::TooManyKeys);
        }
        self.down.push(code);
        Verdict::Pass(event)
    }
}
