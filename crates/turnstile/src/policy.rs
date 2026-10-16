//! Who may have which device. Pure: the caller's facts and the seat's user go in, a yes or a named
//! refusal comes out.
//!
//! | Kind | Role in the table | Strength | User | Control group |
//! |---|---|---|---|---|
//! | palmrest's | `pad_daemon` | image | the seat's active user | a user unit `palmrest.service` of that user's manager |
//! | keycap's | `keymap_daemon` | image | not the seat's active user (it is a dynamic user) | exactly `/system.slice/keycap.service` |
//!
//! A user cannot put a process into `system.slice`, so for keycap the control group is as strong as a
//! root service can get without asking a manager the user controls. For palmrest the image is
//! checked by reading its `/proc/<pid>/exe`, which needs the one capability the unit keeps.

use crate::identity::{ActiveUid, Caller, Credential, SeatSource, Strength};
use peephole_core::roles::{KEYMAP_DAEMON, PAD_DAEMON};
use turnstile_wire::{Kind, Owner};

/// palmrest's unit.
pub const PAD_UNIT: &str = "palmrest.service";
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
    /// Proven by its control group only, or not enough.
    #[error("its image was not checked")]
    TooWeak,
    /// Nobody is logged in, so there is no user whose program this could be.
    #[error("no one is logged in")]
    NoSeat,
    /// Running as a user this program never runs as.
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
    let (role, place) = match kind.owner() {
        Owner::Palmrest => (PAD_DAEMON, Place::UserUnit),
        Owner::Keycap => (KEYMAP_DAEMON, Place::SystemUnit),
        _ => return Err(Refusal::UnknownKind),
    };
    match &caller.credential {
        Credential::Listed { role: have, .. } if *have != role => {
            return Err(Refusal::WrongRole {
                have: have.to_string(),
            });
        }
        Credential::Listed { strength, .. } if *strength < Strength::Image => {
            return Err(Refusal::TooWeak);
        }
        Credential::Listed { .. } => {}
        Credential::Unlisted => return Err(Refusal::Unlisted),
    }
    match place {
        Place::UserUnit => {
            let active = seat.0.ok_or(Refusal::NoSeat)?;
            if caller.uid != active {
                return Err(Refusal::WrongUser);
            }
            if !in_user_unit(&caller.cgroup, active, PAD_UNIT) {
                return Err(Refusal::WrongPlace);
            }
        }
        Place::SystemUnit => {
            if seat.0 == Some(caller.uid) {
                return Err(Refusal::WrongUser);
            }
            if caller.cgroup != KEYMAP_CGROUP {
                return Err(Refusal::WrongPlace);
            }
        }
    }
    Ok(())
}

/// Where a program's control group must be.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// Under the user's own manager.
    UserUnit,
    /// Directly under `system.slice`.
    SystemUnit,
}

/// Whether `cgroup` is `unit` somewhere under `user`'s manager:
/// `/user.slice/user-<uid>.slice/user@<uid>.service/<anything>/<unit>`.
pub fn in_user_unit(cgroup: &str, uid: u32, unit: &str) -> bool {
    let root = format!("/user.slice/user-{uid}.slice/user@{uid}.service/");
    let leaf = cgroup
        .strip_prefix(&root)
        .map(|below| below.rsplit('/').next().unwrap_or(below));
    leaf == Some(unit)
}
