use turnstile::pending::{Caps, Crowded, Pending};

const CAPS: Caps = Caps {
    per_user: 2,
    total: 3,
};

#[test]
fn a_user_has_its_share_and_all_users_a_total_and_a_dropped_ticket_frees_both() {
    let pending = Pending::new(CAPS);
    let a1 = pending.enter(1).expect("first");
    let a2 = pending.enter(1).expect("second");
    assert_eq!(pending.enter(1).err(), Some(Crowded::User));
    let b = pending.enter(2).expect("another user is not affected");
    assert_eq!(pending.enter(3).err(), Some(Crowded::All));
    assert_eq!(pending.count(), 3);
    drop(a1);
    assert_eq!(pending.count(), 2);
    assert!(pending.enter(3).is_ok(), "a freed place is free again");
    drop((a2, b));
}

#[test]
fn counts_return_to_nothing() {
    let pending = Pending::new(CAPS);
    let held: Vec<_> = [1, 1, 2].iter().map(|uid| pending.enter(*uid)).collect();
    assert!(held.iter().all(Result::is_ok));
    drop(held);
    assert_eq!(pending.count(), 0);
    assert!(pending.enter(1).is_ok() && pending.enter(1).is_ok());
}
