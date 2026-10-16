//! The one test that makes a real device. It needs `/dev/uinput` and is never run in the jail: it is
//! on the owner's manual queue (`sill/docs/manual-checks.md`).

use std::time::{Duration, Instant};
use turnstile::backend::real::UinputBackend;
use turnstile::backend::{Backend, Device};
use turnstile_wire::codes::{ABS_MT_SLOT, EV_ABS};
use turnstile_wire::{Create, Event, plan};

/// Whether an input device called `name` is registered with the kernel.
fn registered(name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/input") else {
        return false;
    };
    entries
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("name")).ok())
        .any(|n| n.trim() == name)
}

fn wait_for(name: &str, present: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if registered(name) == present {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
#[ignore = "manual: makes a real device through /dev/uinput; run by the owner, never in the jail"]
fn a_real_touchpad_appears_takes_a_frame_and_disappears() {
    let spec = plan(&Create::touchpad()).expect("plans").spec;
    let mut device = UinputBackend::default().create(&spec).expect("a touchpad");
    assert!(
        wait_for("palmrest touchpad", true),
        "the device did not appear"
    );
    device
        .write(&[Event::new(EV_ABS, ABS_MT_SLOT, 0), Event::SYNC])
        .expect("a frame");
    drop(device);
    assert!(
        wait_for("palmrest touchpad", false),
        "the device did not go"
    );
}

#[test]
#[ignore = "manual: makes a real device through /dev/uinput; run by the owner, never in the jail"]
fn a_real_keyboard_with_lights_is_handed_over_as_a_descriptor() {
    let spec = plan(&Create::keyboard()).expect("plans").spec;
    let device = UinputBackend::default().create(&spec).expect("a keyboard");
    assert!(
        wait_for("keycap keyboard", true),
        "the device did not appear"
    );
    let fd = device.into_fd();
    assert!(
        wait_for("keycap keyboard", true),
        "it stays while the descriptor lives"
    );
    drop(fd);
    assert!(
        wait_for("keycap keyboard", false),
        "it goes with the descriptor"
    );
}
