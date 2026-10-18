//! keycap's two handed devices: fixed shapes, and a pointer that can never be a keyboard.

use turnstile_wire::ceiling::{KEYBOARD_NAME, POINTER_SUFFIX};
use turnstile_wire::codes::*;
use turnstile_wire::{AbsAxis, BadCaps, Create, PointerCaps, Route, plan};

#[test]
fn the_keyboard_is_the_full_keyboard_of_keycap() {
    let p = plan(&Create::keyboard()).expect("plans");
    assert!(
        matches!(p.route, Route::Handed),
        "handed devices are not filtered"
    );
    let spec = p.spec;
    assert_eq!(spec.name, KEYBOARD_NAME);
    assert_eq!((spec.vendor, spec.product), (0x4b43, 0x0001));
    assert_eq!(spec.keys.first(), Some(&1));
    assert_eq!(spec.keys.last(), Some(&KEY_MAX));
    assert_eq!(spec.keys.len(), usize::from(KEY_MAX));
    assert_eq!(
        spec.rels,
        [
            REL_X,
            REL_Y,
            REL_WHEEL,
            REL_HWHEEL,
            REL_WHEEL_HI_RES,
            REL_HWHEEL_HI_RES
        ]
    );
    assert_eq!(spec.leds, [LED_NUML, LED_CAPSL, LED_SCROLLL]);
    assert!(spec.abs.is_empty() && spec.props.is_empty());
    assert_eq!(spec.event_types(), [EV_KEY, EV_REL, EV_LED]);
}

fn axis(code: u16, minimum: i32, maximum: i32) -> AbsAxis {
    AbsAxis {
        code,
        minimum,
        maximum,
        fuzz: 0,
        flat: 0,
        resolution: 12,
    }
}

fn trackpad() -> PointerCaps {
    PointerCaps {
        label: "Apple Magic Trackpad".to_owned(),
        vendor: 0x05ac,
        product: 0x0265,
        props: vec![0, 2],
        buttons: vec![BTN_LEFT, BTN_TOUCH, BTN_TOOL_FINGER],
        rels: vec![],
        abs: vec![axis(ABS_X, 0, 6000), axis(ABS_Y, 0, 4000)],
    }
}

#[test]
fn a_pointer_keeps_what_the_original_had_and_gains_the_suffix() {
    let spec = plan(&Create::pointer(trackpad())).expect("plans").spec;
    assert_eq!(spec.name, format!("Apple Magic Trackpad {POINTER_SUFFIX}"));
    assert_eq!((spec.vendor, spec.product), (0x05ac, 0x0265));
    assert_eq!(spec.keys, [BTN_LEFT, BTN_TOOL_FINGER, BTN_TOUCH]);
    assert_eq!(spec.abs.len(), 2);
    assert_eq!(spec.props, [0, 2]);
    assert!(spec.leds.is_empty());
}

#[test]
fn a_pointer_never_registers_a_key() {
    let caps = PointerCaps {
        buttons: vec![
            30,
            1,
            0xff,
            0x160,
            0x2bf,
            BTN_LEFT,
            0x2c0,
            0x2ff,
            0x300,
            u16::MAX,
        ],
        rels: vec![REL_X, 0x10, 0x20],
        props: vec![0, 5, 6, 31],
        ..trackpad()
    };
    let spec = plan(&Create::pointer(caps)).expect("plans").spec;
    assert_eq!(spec.keys, [BTN_LEFT, 0x2c0, 0x2ff]);
    assert_eq!(spec.rels, [REL_X]);
    assert_eq!(spec.props, [0, 5]);
}

#[test]
fn the_name_always_fits_and_ends_with_the_suffix() {
    let long = PointerCaps {
        label: "é".repeat(31),
        ..trackpad()
    };
    let name = plan(&Create::pointer(long)).expect("plans").spec.name;
    assert!(name.len() < 80, "{} bytes", name.len());
    assert!(name.ends_with(POINTER_SUFFIX));
    let empty = PointerCaps {
        label: String::new(),
        ..trackpad()
    };
    let name = plan(&Create::pointer(empty)).expect("plans").spec.name;
    assert_eq!(name, format!(" {POINTER_SUFFIX}"));
}

#[test]
fn pointers_outside_the_ceiling_are_refused() {
    let nothing = PointerCaps {
        buttons: vec![30],
        rels: vec![0x20],
        abs: vec![],
        ..trackpad()
    };
    let cases: Vec<(&str, PointerCaps, BadCaps)> = vec![
        ("nothing a pointer does", nothing, BadCaps::EmptyPointer),
        (
            "axis code past ABS_MAX",
            PointerCaps {
                abs: vec![axis(0x40, 0, 1)],
                ..trackpad()
            },
            BadCaps::BadAxis(0x40),
        ),
        (
            "minimum above maximum",
            PointerCaps {
                abs: vec![axis(0, 5, 1)],
                ..trackpad()
            },
            BadCaps::BadAxis(0),
        ),
        (
            "huge range",
            PointerCaps {
                abs: vec![axis(1, 0, i32::MAX)],
                ..trackpad()
            },
            BadCaps::BadAxis(1),
        ),
        (
            "negative resolution",
            PointerCaps {
                abs: vec![AbsAxis {
                    resolution: -1,
                    ..axis(0, 0, 10)
                }],
                ..trackpad()
            },
            BadCaps::BadAxis(0),
        ),
    ];
    for (name, caps, want) in cases {
        assert_eq!(plan(&Create::pointer(caps)).err(), Some(want), "{name}");
    }
}

#[test]
fn duplicate_axes_collapse_to_the_first() {
    let caps = PointerCaps {
        abs: vec![axis(ABS_X, 0, 10), axis(ABS_X, 0, 99)],
        ..trackpad()
    };
    let spec = plan(&Create::pointer(caps)).expect("plans").spec;
    assert_eq!(spec.abs, [axis(ABS_X, 0, 10)]);
}
