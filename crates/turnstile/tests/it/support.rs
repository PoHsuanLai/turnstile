//! Callers, seats and contexts the tests share.

use peephole_core::RoleName;
use std::sync::Arc;
use turnstile::backend::fake::FakeBackend;
use turnstile::clock::ManualClock;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::identity::{ActiveUid, Caller, Credential, Strength};
use turnstile::serve::Context;

/// The user at the seat.
pub(crate) const USER: u32 = 1000;
/// keycap's dynamic user.
pub(crate) const KEYCAP_UID: u32 = 61_234;

pub(crate) fn listed(role: &str, strength: Strength, uid: u32, cgroup: &str) -> Caller {
    let role = RoleName::new(role).expect("a valid role name");
    Caller {
        pid: 4242,
        uid,
        cgroup: cgroup.to_owned(),
        image: Some("/usr/local/bin/x".to_owned()),
        credential: Credential::Listed { role, strength },
    }
}

pub(crate) fn palmrest_cgroup(uid: u32) -> String {
    format!("/user.slice/user-{uid}.slice/user@{uid}.service/app.slice/palmrest.service")
}

/// The genuine palmrest of `uid`.
pub(crate) fn palmrest(uid: u32) -> Caller {
    listed("pad_daemon", Strength::Image, uid, &palmrest_cgroup(uid))
}

/// The genuine keycap.
pub(crate) fn keycap() -> Caller {
    listed(
        "keymap_daemon",
        Strength::Image,
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
