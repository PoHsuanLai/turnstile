use turnstile_wire::{Delivery, Kind, Owner};

#[test]
fn every_kind_round_trips_its_code_and_has_one_owner_and_delivery() {
    const WANT: &[(Kind, u8, Owner, Delivery)] = &[
        (
            Kind::PalmrestTouchpad,
            1,
            Owner::Palmrest,
            Delivery::Proxied,
        ),
        (Kind::PalmrestWheel, 2, Owner::Palmrest, Delivery::Proxied),
        (Kind::PalmrestKeys, 3, Owner::Palmrest, Delivery::Proxied),
        (Kind::KeycapKeyboard, 16, Owner::Keycap, Delivery::Handed),
        (Kind::KeycapPointer, 17, Owner::Keycap, Delivery::Handed),
    ];
    assert_eq!(WANT.len(), Kind::ALL.len());
    for &(kind, code, owner, delivery) in WANT {
        assert_eq!(kind.code(), code, "{kind:?}");
        assert_eq!(Kind::from_code(code), Some(kind));
        assert_eq!(kind.owner(), owner, "{kind:?}");
        assert_eq!(kind.delivery(), delivery, "{kind:?}");
    }
}

#[test]
fn unassigned_codes_are_no_kind() {
    let known: Vec<u8> = Kind::ALL.iter().map(|k| k.code()).collect();
    for code in 0..=u8::MAX {
        assert_eq!(
            Kind::from_code(code).is_some(),
            known.contains(&code),
            "{code}"
        );
    }
}
