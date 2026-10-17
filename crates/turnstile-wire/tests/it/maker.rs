use turnstile_wire::codes::{BTN_LEFT, REL_X, REL_Y};
use turnstile_wire::{Create, Kind, Maker, PointerCaps, plan};

/// One request for every kind, valid for its ceiling.
fn create_of(kind: Kind) -> Create {
    match kind {
        Kind::PalmrestTouchpad => Create::touchpad(),
        Kind::PalmrestWheel => Create::wheel(),
        Kind::PalmrestKeys => Create::keys(vec![30, 29]),
        Kind::KeycapKeyboard => Create::keyboard(),
        Kind::KeycapPointer => Create::pointer(PointerCaps {
            buttons: vec![BTN_LEFT],
            rels: vec![REL_X, REL_Y],
            ..PointerCaps::default()
        }),
        _ => unreachable!("a kind this test does not know"),
    }
}

/// casement keeps its own copy of the parser and pins these same strings (its `origin.rs` test).
/// Change one side and the other must change with it.
#[test]
fn the_labels_are_these_exact_strings_and_parse_back() {
    const WANT: &[(Maker, &str)] = &[
        (Maker::PadDaemon, "turnstile/pad_daemon"),
        (Maker::KeymapDaemon, "turnstile/keymap_daemon"),
    ];
    for &(maker, label) in WANT {
        assert_eq!(maker.to_string(), label);
        assert_eq!(Maker::parse(label), Some(maker));
        assert_eq!(
            Maker::parse(&format!("{label}\n")),
            Some(maker),
            "sysfs adds a newline"
        );
    }
}

#[test]
fn any_other_text_is_no_maker() {
    for text in [
        "",
        "turnstile/",
        "turnstile/pad_daemon/extra",
        "turnstile/pad_daemon ",
        "turnstile/Pad_daemon",
        "turnstile/settings",
        "turnstile",
        "xturnstile/pad_daemon",
        "ydotool virtual device",
        "usb-0000:00:14.0-2/input0",
    ] {
        assert_eq!(Maker::parse(text), None, "{text:?}");
    }
}

#[test]
fn every_kind_is_labelled_with_the_service_it_is_made_for() {
    for kind in Kind::ALL {
        let spec = plan(&create_of(kind)).expect("plans").spec;
        assert_eq!(spec.maker, Maker::of(kind), "{kind:?}");
    }
    assert_eq!(Maker::of(Kind::PalmrestTouchpad), Maker::PadDaemon);
    assert_eq!(Maker::of(Kind::PalmrestWheel), Maker::PadDaemon);
    assert_eq!(Maker::of(Kind::PalmrestKeys), Maker::PadDaemon);
    assert_eq!(Maker::of(Kind::KeycapKeyboard), Maker::KeymapDaemon);
    assert_eq!(Maker::of(Kind::KeycapPointer), Maker::KeymapDaemon);
}
