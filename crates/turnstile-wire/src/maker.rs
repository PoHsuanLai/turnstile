//! Whom a device was made for, written into the device itself.
//!
//! The kernel keeps a free-text "physical path" (`phys`) on every input device and shows it in
//! `/sys/class/input/<event>/device/phys`. Only a process that can open `/dev/uinput` can set it, and
//! that is turnstile alone (its udev rule makes the node root-only), so a desktop that reads
//! `turnstile/pad_daemon` there knows which proven service the device was made for. The form is
//! `turnstile/<role>`, the role being the peephole role the service admitted the caller under.
//!
//! casement keeps its own copy of [`Maker::parse`] (it links nothing of turnstile); each repository
//! pins the exact strings in a test, so a change here fails both.

use crate::kind::{Kind, Owner};
use std::fmt;

/// What comes before the role in a label.
pub const PREFIX: &str = "turnstile/";

/// The proven service a device was made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Maker {
    /// palmrest, the Magic Mouse gesture daemon: peephole role `pad_daemon`.
    PadDaemon,
    /// keycap, the Mac keyboard mapping daemon: peephole role `keymap_daemon`.
    KeymapDaemon,
}

impl Maker {
    /// The service every device of `kind` is made for.
    pub const fn of(kind: Kind) -> Self {
        match kind.owner() {
            Owner::Palmrest => Self::PadDaemon,
            Owner::Keycap => Self::KeymapDaemon,
        }
    }

    /// The peephole role name.
    pub const fn role(self) -> &'static str {
        match self {
            Self::PadDaemon => "pad_daemon",
            Self::KeymapDaemon => "keymap_daemon",
        }
    }

    /// The maker a `phys` text names, or `None` for any other text (no label, another prefix, a role
    /// this version does not know, anything after the role). A trailing newline, as sysfs adds, is
    /// ignored.
    pub fn parse(phys: &str) -> Option<Self> {
        let role = phys.trim_end_matches('\n').strip_prefix(PREFIX)?;
        [Self::PadDaemon, Self::KeymapDaemon]
            .into_iter()
            .find(|maker| maker.role() == role)
    }
}

/// The label: `turnstile/<role>`.
impl fmt::Display for Maker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}", self.role())
    }
}
