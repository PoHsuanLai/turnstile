use std::fmt::Display;
use turnstile::activation::{ActivationError, listener};
use turnstile::clock::{Clock, ManualClock};
use turnstile::idle::{Idle, Idling};
use turnstile::journal::{kv, line};
use turnstile_wire::Millis;

#[test]
fn the_service_leaves_after_the_limit_with_no_connection_and_not_before() {
    let mut idle = Idle::new(60_000);
    let steps: &[(u64, usize, Idling)] = &[
        (0, 0, Idling::Stay),
        (30_000, 0, Idling::Stay),
        (59_999, 0, Idling::Stay),
        (60_000, 0, Idling::Exit),
    ];
    for &(ms, live, want) in steps {
        assert_eq!(idle.observe(Millis(ms), live), want, "at {ms} ms");
    }
}

#[test]
fn a_connection_restarts_the_count() {
    let mut idle = Idle::new(1_000);
    assert_eq!(idle.observe(Millis(0), 0), Idling::Stay);
    assert_eq!(idle.observe(Millis(900), 1), Idling::Stay);
    // The connection ends at 5000: the minute starts there, not at the first look.
    assert_eq!(idle.observe(Millis(5_000), 0), Idling::Stay);
    assert_eq!(idle.observe(Millis(5_999), 0), Idling::Stay);
    assert_eq!(idle.observe(Millis(6_000), 0), Idling::Exit);
    assert_eq!(
        idle.observe(Millis(2), 3),
        Idling::Stay,
        "time going back is no panic"
    );
}

#[test]
fn the_manual_clock_moves_only_when_told() {
    let clock = ManualClock::new();
    assert_eq!(clock.now(), Millis(0));
    clock.advance(250);
    clock.advance(5);
    assert_eq!(clock.now(), Millis(255));
}

#[test]
fn log_lines_are_one_line_with_quoted_odd_values() {
    let pid: &dyn Display = &42;
    let why: &dyn Display = &"events for a device\nthat is not there";
    let empty: &dyn Display = &"";
    let got = line(
        "refused",
        &[kv("pid", pid), kv("why", why), kv("empty", empty)],
    );
    assert_eq!(
        got,
        r#"turnstile: refused pid=42 why="events for a device\nthat is not there" empty="""#
    );
    assert!(!got.contains('\n'));
}

#[test]
fn started_by_hand_it_says_to_use_the_socket_unit() {
    // The test process has no socket passed in.
    let err = listener().expect_err("no socket");
    assert!(matches!(err, ActivationError::NotActivated), "{err:?}");
}
