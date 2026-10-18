//! Callers, seats and contexts the tests share.

use peephole_core::{CgroupPath, ExePath, Pid, Proof, RoleName, Uid};
use std::sync::Arc;
use turnstile::backend::fake::FakeBackend;
use turnstile::clock::ManualClock;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::identity::{ActiveUid, Caller, Credential};
use turnstile::serve::Context;

/// The user at the seat.
pub(crate) const USER: Uid = Uid::new(1000);
/// keycap's dynamic user.
pub(crate) const KEYCAP_UID: Uid = Uid::new(61_234);
/// palmrest's dynamic user.
pub(crate) const PALMREST_UID: Uid = Uid::new(61_235);
/// palmrest's control group: a system service's.
pub(crate) const PALMREST_CGROUP: &str = "/system.slice/palmrest.service";

pub(crate) fn listed(role: &str, proof: Proof, uid: Uid, cgroup: &str) -> Caller {
    let role = RoleName::new(role).expect("a valid role name");
    Caller {
        pid: Pid::new(4242).expect("a process number"),
        uid,
        cgroup: CgroupPath::new(cgroup).expect("a cgroup path"),
        image: Some(ExePath::new("/usr/local/bin/x").expect("an image path")),
        credential: Credential::Listed { role, proof },
    }
}

/// The genuine palmrest.
pub(crate) fn palmrest() -> Caller {
    listed("pad_daemon", Proof::Full, PALMREST_UID, PALMREST_CGROUP)
}

/// The genuine keycap.
pub(crate) fn keycap() -> Caller {
    listed(
        "keymap_daemon",
        Proof::Full,
        KEYCAP_UID,
        "/system.slice/keycap.service",
    )
}

pub(crate) type TestContext = Context<FakeBackend, FixedAuthority, FixedSeat, ManualClock>;

/// A context where every peer is `caller` and `USER` is at the seat.
pub(crate) fn context(caller: Caller) -> (Arc<TestContext>, FakeBackend) {
    let backend = FakeBackend::new();
    let ctx = Context {
        backend: backend.clone(),
        authority: FixedAuthority::naming(caller),
        seat: FixedSeat(ActiveUid(Some(USER))),
        clock: ManualClock::new(),
    };
    (Arc::new(ctx), backend)
}
