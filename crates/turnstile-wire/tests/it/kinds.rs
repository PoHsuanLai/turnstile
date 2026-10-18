use turnstile_wire::{Delivery, Kind, Maker};

#[test]
fn every_kind_round_trips_its_code_and_has_one_maker_and_delivery() {
    const WANT: &[(Kind, u8, Maker, Delivery)] = &[
        (
            Kind::PalmrestTouchpad,
            1,
            Maker::PadDaemon,
            Delivery::Proxied,
        ),
        (Kind::PalmrestWheel, 2, Maker::PadDaemon, Delivery::Proxied),
        (Kind::PalmrestKeys, 3, Maker::PadDaemon, Delivery::Proxied),
        (
            Kind::KeycapKeyboard,
            16,
            Maker::KeymapDaemon,
            Delivery::Handed,
        ),
        (
            Kind::KeycapPointer,
            17,
            Maker::KeymapDaemon,
            Delivery::Handed,
        ),
    ];
    assert_eq!(WANT.len(), Kind::ALL.len());
    for &(kind, code, maker, delivery) in WANT {
        assert_eq!(kind.code(), code, "{kind:?}");
        assert_eq!(Kind::from_code(code), Some(kind));
        assert_eq!(kind.maker(), maker, "{kind:?}");
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
