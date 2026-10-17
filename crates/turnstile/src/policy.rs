//! Who may have which device. Pure: the caller's facts and the seat's user go in, a yes or a named
//! refusal comes out.
//!
//! | Kind | Role in the table | Strength | User | Control group |
//! |---|---|---|---|---|
//! | palmrest's | `pad_daemon` | unit | not the seat's active user (it is a dynamic user) | exactly `/system.slice/palmrest.service` |
//! | keycap's | `keymap_daemon` | unit | not the seat's active user (it is a dynamic user) | exactly `/system.slice/keycap.service` |
//!
//! Both are system services, so the control group is one only the system manager can fill; both are
//! served before anyone is logged in.
//!
//! "Unit" is peephole's `Full`: a root-owned image in the table, a unit with a root-owned fragment
//! and drop-ins that unsets every loader variable, and a process nothing traces. A control group name or an image alone is refused (`TooWeak`) and logged: a process of
//! the same user can make the first and run the second under a preloaded library. The checks that
//! need another user's `/proc/<pid>` use the one capability the unit keeps.

use crate::identity::{ActiveUid, Caller, Credential, SeatSource, Strength};
use peephole_core::roles::{KEYMAP_DAEMON, PAD_DAEMON};
use turnstile_wire::{Kind, Owner};

/// palmrest's control group, which only a system service can have.
pub const PAD_CGROUP: &str = "/system.slice/palmrest.service";
/// keycap's control group, which only a system service can have.
pub const KEYMAP_CGROUP: &str = "/system.slice/keycap.service";

/// Why a caller may not have a device.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The caller is not a listed desktop program, or was not proven to be one.
    #[error("not a listed desktop program")]
    Unlisted,
    /// A listed program with another job.
    #[error("the role {have} does not take this device")]
    WrongRole {
        /// The role the table gave.
        have: String,
    },
    /// Proven by its control group or its image only: its unit was not shown to be the real one.
    #[error("its unit was not checked")]
    TooWeak,
    /// Running as the user at the seat, which these programs (dynamic users) never are.
    #[error("running as the wrong user")]
    WrongUser,
    /// Running in a control group this program never runs in.
    #[error("running in the wrong place")]
    WrongPlace,
    /// A device kind this version does not know.
    #[error("a device this version does not know")]
    UnknownKind,
}

/// Decides, for one connection, which kinds the caller may ask for.
pub trait Admission {
    /// Whether the caller may have a device of `kind` now.
    fn admit(&self, kind: Kind) -> Result<(), Refusal>;
}

/// The admission of a real caller, judged against the seat as it is at each request.
#[derive(Debug)]
pub struct CallerAdmission<'a, S> {
    /// The caller.
    pub caller: &'a Caller,
    /// Where the seat's user comes from.
    pub seat: &'a S,
}

impl<S: SeatSource> Admission for CallerAdmission<'_, S> {
    fn admit(&self, kind: Kind) -> Result<(), Refusal> {
        authorise(kind, self.caller, self.seat.active_uid())
    }
}

/// An admission that allows the listed kinds, for tests that are not about identity.
#[derive(Debug, Clone)]
pub struct AllowKinds(pub Vec<Kind>);

impl Admission for AllowKinds {
    fn admit(&self, kind: Kind) -> Result<(), Refusal> {
        if self.0.contains(&kind) {
            Ok(())
        } else {
            Err(Refusal::Unlisted)
        }
    }
}

/// Whether `caller` may have a device of `kind` while `seat`'s user is active.
pub fn authorise(kind: Kind, caller: &Caller, seat: ActiveUid) -> Result<(), Refusal> {
    let (role, cgroup) = match kind.owner() {
        Owner::Palmrest => (PAD_DAEMON, PAD_CGROUP),
        Owner::Keycap => (KEYMAP_DAEMON, KEYMAP_CGROUP),
        _ => return Err(Refusal::UnknownKind),
    };
    match &caller.credential {
        Credential::Listed { role: have, .. } if *have != role => {
            return Err(Refusal::WrongRole {
                have: have.to_string(),
            });
        }
        Credential::Listed { strength, .. } if *strength < Strength::Unit => {
            return Err(Refusal::TooWeak);
        }
        Credential::Listed { .. } => {}
        Credential::Unlisted => return Err(Refusal::Unlisted),
    }
    if seat.0 == Some(caller.uid) {
        return Err(Refusal::WrongUser);
    }
    if caller.cgroup != cgroup {
        return Err(Refusal::WrongPlace);
    }
    Ok(())
}
