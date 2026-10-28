//! palmrest's touchpad and wheel: what palmrest really writes passes unchanged, and nothing else does.

use crate::{abs, key, rel, sync};
use turnstile_wire::codes::*;
use turnstile_wire::{Create, Denial, Event, Filter, Route, plan};

fn filter(create: &Create) -> Filter {
    let Route::Proxied(filter) = plan(create).expect("within the ceiling").route else {
        panic!("a proxied kind");
    };
    filter
}

fn pass_all(f: &mut Filter, events: &[Event]) {
    let out = f.apply(events);
    assert!(out.denied.is_empty(), "{events:?}: {:?}", out.denied);
    assert_eq!(out.pass, events);
}

/// The sequences palmrest's `PadEncoder` writes for a two finger land, a move and a lift.
#[test]
fn what_palmrest_writes_for_a_two_finger_gesture_passes_unchanged() {
    let mut f = filter(&Create::touchpad());
    pass_all(
        &mut f,
        &[
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, 7),
            abs(ABS_MT_POSITION_X, 1600),
            abs(ABS_MT_POSITION_Y, 1400),
            abs(ABS_MT_SLOT, 1),
            abs(ABS_MT_TRACKING_ID, 8),
            abs(ABS_MT_POSITION_X, 2400),
            abs(ABS_MT_POSITION_Y, 1400),
            key(BTN_TOUCH, 1),
            key(BTN_TOOL_DOUBLETAP, 1),
            abs(ABS_X, 1600),
            abs(ABS_Y, 1400),
            sync(),
        ],
    );
    pass_all(
        &mut f,
        &[
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_POSITION_X, 1700),
            abs(ABS_MT_POSITION_Y, 1500),
            abs(ABS_X, 1700),
            abs(ABS_Y, 1500),
            sync(),
        ],
    );
    pass_all(
        &mut f,
        &[
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, -1),
            abs(ABS_MT_SLOT, 1),
            abs(ABS_MT_TRACKING_ID, -1),
            key(BTN_TOUCH, 0),
            key(BTN_TOOL_DOUBLETAP, 0),
            sync(),
        ],
    );
}

#[test]
fn touchpad_events_table() {
    use Denial::*;
    // (event, what a fresh touchpad says): Ok(written) or Err(denial).
    let table: &[(&str, Event, Result<Event, Denial>)] = &[
        ("sync", sync(), Ok(sync())),
        (
            "sync with a value",
            Event::new(EV_SYN, SYN_REPORT, 1),
            Err(NotAllowed),
        ),
        ("a letter key", key(30, 1), Err(NotAllowed)),
        ("the middle button", key(0x112, 1), Err(NotAllowed)),
        ("a relative move", rel(REL_X, 5), Err(NotAllowed)),
        ("a wheel step", rel(REL_WHEEL, 1), Err(NotAllowed)),
        ("a pressure axis", abs(0x18, 10), Err(NotAllowed)),
        ("a light", Event::new(EV_LED, 1, 1), Err(NotAllowed)),
        (
            "a click with no finger down",
            key(BTN_LEFT, 1),
            Err(NoContact),
        ),
        ("a click release", key(BTN_LEFT, 0), Ok(key(BTN_LEFT, 0))),
        ("a tool with value 2", key(BTN_TOUCH, 2), Err(BadValue)),
        (
            "a tool with value -1",
            key(BTN_TOOL_FINGER, -1),
            Err(BadValue),
        ),
        ("x inside", abs(ABS_X, 4000), Ok(abs(ABS_X, 4000))),
        (
            "x too far is clamped",
            abs(ABS_X, 9000),
            Ok(abs(ABS_X, 4000)),
        ),
        (
            "x negative is clamped",
            abs(ABS_MT_POSITION_X, -5),
            Ok(abs(ABS_MT_POSITION_X, 0)),
        ),
        (
            "y too far is clamped",
            abs(ABS_MT_POSITION_Y, 2801),
            Ok(abs(ABS_MT_POSITION_Y, 2800)),
        ),
        ("slot 4", abs(ABS_MT_SLOT, 4), Ok(abs(ABS_MT_SLOT, 4))),
        ("slot 5", abs(ABS_MT_SLOT, 5), Err(OutOfRange)),
        ("slot -1", abs(ABS_MT_SLOT, -1), Err(OutOfRange)),
        (
            "tracking id 0xffff",
            abs(ABS_MT_TRACKING_ID, 0xffff),
            Ok(abs(ABS_MT_TRACKING_ID, 0xffff)),
        ),
        (
            "tracking id lift",
            abs(ABS_MT_TRACKING_ID, -1),
            Ok(abs(ABS_MT_TRACKING_ID, -1)),
        ),
        (
            "tracking id too big",
            abs(ABS_MT_TRACKING_ID, 0x10000),
            Err(OutOfRange),
        ),
        (
            "tracking id -2",
            abs(ABS_MT_TRACKING_ID, -2),
            Err(OutOfRange),
        ),
    ];
    for (name, event, want) in table {
        let mut f = filter(&Create::touchpad());
        let got = f.check(*event);
        assert_eq!(&got, want, "{name}");
    }
}

#[test]
fn a_click_needs_a_finger_down_and_ends_when_it_lifts() {
    let mut f = filter(&Create::touchpad());
    assert_eq!(f.check(key(BTN_LEFT, 1)), Err(Denial::NoContact));
    pass_all(&mut f, &[abs(ABS_MT_SLOT, 2), abs(ABS_MT_TRACKING_ID, 1)]);
    assert_eq!(f.check(key(BTN_LEFT, 1)), Ok(key(BTN_LEFT, 1)));
    pass_all(&mut f, &[abs(ABS_MT_TRACKING_ID, -1)]);
    assert_eq!(f.check(key(BTN_LEFT, 1)), Err(Denial::NoContact));
}

#[test]
fn the_touchpad_capabilities_are_exactly_palmrests() {
    let spec = plan(&Create::touchpad()).expect("plans").spec;
    assert_eq!(spec.name, "palmrest touchpad");
    assert_eq!((spec.vendor, spec.product), (0x0001, 0x0002));
    assert_eq!(
        spec.keys,
        [
            BTN_LEFT,
            BTN_TOUCH,
            BTN_TOOL_FINGER,
            BTN_TOOL_DOUBLETAP,
            BTN_TOOL_TRIPLETAP,
            BTN_TOOL_QUADTAP
        ]
    );
    assert!(spec.rels.is_empty() && spec.leds.is_empty());
    assert_eq!(spec.props, [INPUT_PROP_POINTER]);
    let axes: Vec<(u16, i32, i32, i32)> = spec
        .abs
        .iter()
        .map(|a| (a.code, a.minimum, a.maximum, a.resolution))
        .collect();
    assert_eq!(
        axes,
        [
            (ABS_X, 0, 4000, 40),
            (ABS_Y, 0, 2800, 40),
            (ABS_MT_SLOT, 0, 4, 0),
            (ABS_MT_TRACKING_ID, 0, 0xffff, 0),
            (ABS_MT_POSITION_X, 0, 4000, 40),
            (ABS_MT_POSITION_Y, 0, 2800, 40),
        ]
    );
    assert_eq!(spec.event_types(), [EV_KEY, EV_ABS]);
}

#[test]
fn the_wheel_passes_its_four_axes_up_to_the_magnitude_cap() {
    let mut f = filter(&Create::wheel());
    pass_all(
        &mut f,
        &[
            rel(REL_WHEEL, -1),
            rel(REL_WHEEL_HI_RES, -120),
            rel(REL_HWHEEL, 3),
            rel(REL_HWHEEL_HI_RES, 1200),
            sync(),
        ],
    );
    let denied = f.apply(&[
        rel(REL_WHEEL_HI_RES, 1201),
        rel(REL_WHEEL_HI_RES, -1201),
        rel(REL_WHEEL_HI_RES, i32::MIN),
        rel(REL_X, 1),
        rel(REL_Y, 1),
        key(BTN_LEFT, 1),
        abs(ABS_X, 1),
        Event::new(EV_LED, 0, 1),
    ]);
    assert!(denied.pass.is_empty(), "{denied:?}");
    assert_eq!(
        denied.denied,
        [
            Denial::OutOfRange,
            Denial::OutOfRange,
            Denial::OutOfRange,
            Denial::NotAllowed,
            Denial::NotAllowed,
            Denial::NotAllowed,
            Denial::NotAllowed,
            Denial::NotAllowed,
        ]
    );
}

#[test]
fn the_wheel_cannot_click_or_point() {
    let spec = plan(&Create::wheel()).expect("plans").spec;
    assert_eq!(spec.name, "palmrest wheel");
    assert_eq!(
        spec.rels,
        [REL_WHEEL, REL_HWHEEL, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES]
    );
    assert!(spec.keys.is_empty() && spec.abs.is_empty() && spec.props.is_empty());
    assert_eq!(spec.event_types(), [EV_REL]);
}
