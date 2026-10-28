use crate::support::{KEYCAP_UID, PALMREST_UID, USER, keycap, palmrest};
use peephole_core::Uid;
use turnstile::identity::{ActiveUid, Caller};
use turnstile::slots::{Bucket, NoSlot, Slots};
use turnstile_wire::Maker;
use turnstile_wire::limits::{MAX_CONNECTIONS, MAX_CONNECTIONS_PER_USER, MAX_RESERVED_PER_CLIENT};

fn seat() -> ActiveUid {
    ActiveUid(Some(USER))
}

fn stranger(uid: Uid) -> Caller {
    Caller {
        uid,
        credential: None,
        ..palmrest()
    }
}

#[test]
fn a_caller_that_passes_the_policy_has_a_reserve_and_anyone_else_a_user_pool() {
    let table: &[(&str, Caller, Bucket)] = &[
        ("palmrest", palmrest(), Bucket::Reserved(Maker::PadDaemon)),
        ("keycap", keycap(), Bucket::Reserved(Maker::KeymapDaemon)),
        (
            "a stranger at the seat",
            stranger(USER),
            Bucket::Stranger(USER),
        ),
        (
            "a stranger elsewhere",
            stranger(Uid::new(2000)),
            Bucket::Stranger(Uid::new(2000)),
        ),
        (
            "palmrest's user is not a free pass",
            Caller {
                credential: None,
                ..palmrest()
            },
            Bucket::Stranger(PALMREST_UID),
        ),
        (
            "palmrest running as the person at the seat",
            Caller {
                uid: USER,
                ..palmrest()
            },
            Bucket::Stranger(USER),
        ),
        (
            "keycap's user is not a free pass",
            Caller {
                credential: None,
                ..keycap()
            },
            Bucket::Stranger(KEYCAP_UID),
        ),
    ];
    for (name, caller, want) in table {
        assert_eq!(Bucket::of(caller, seat()), *want, "{name}");
    }
}

#[test]
fn one_user_cannot_take_another_users_slots_or_the_reserve() {
    let slots = Slots::new();
    let mine: Vec<_> = (0..MAX_CONNECTIONS_PER_USER)
        .map(|_| slots.take(Bucket::Stranger(Uid::new(1))).expect("a slot"))
        .collect();
    assert_eq!(
        slots.take(Bucket::Stranger(Uid::new(1))).err(),
        Some(NoSlot)
    );
    let theirs = slots
        .take(Bucket::Stranger(Uid::new(2)))
        .expect("another user's slot");
    let pad = slots
        .take(Bucket::Reserved(Maker::PadDaemon))
        .expect("reserve");
    assert_eq!(slots.open(), MAX_CONNECTIONS_PER_USER + 2);
    drop(mine);
    assert_eq!(slots.open(), 2);
    assert!(
        slots.take(Bucket::Stranger(Uid::new(1))).is_ok(),
        "a dropped slot is free again"
    );
    drop((theirs, pad));
}

#[test]
fn many_users_share_the_stranger_total_but_never_the_reserve() {
    let slots = Slots::new();
    let held: Vec<_> = (0..MAX_CONNECTIONS)
        .map(|uid| {
            slots
                .take(Bucket::Stranger(Uid::new(
                    u32::try_from(uid).expect("small"),
                )))
                .expect("a slot")
        })
        .collect();
    assert_eq!(
        slots.take(Bucket::Stranger(Uid::new(9999))).err(),
        Some(NoSlot)
    );
    for client in [Maker::PadDaemon, Maker::KeymapDaemon] {
        let reserve: Vec<_> = (0..MAX_RESERVED_PER_CLIENT)
            .map(|_| slots.take(Bucket::Reserved(client)).expect("reserve"))
            .collect();
        assert_eq!(slots.take(Bucket::Reserved(client)).err(), Some(NoSlot));
        drop(reserve);
    }
    drop(held);
    assert_eq!(slots.open(), 0);
}
