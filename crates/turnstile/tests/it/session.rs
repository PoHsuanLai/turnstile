use peephole_core::roles::{KEYMAP_DAEMON, PAD_DAEMON};
use turnstile::backend::fake::{FakeBackend, Fate};
use turnstile::policy::AllowKinds;
use turnstile::session::{Handled, Outgoing, Refused, Session};
use turnstile_wire::codes::*;
use turnstile_wire::limits::{
    EVENT_BURST, FLOOD_CUT_MS, MAX_CREATES_PER_CONNECTION, MAX_DENIED_EVENTS,
    MAX_DEVICES_PER_CONNECTION, MAX_HANDED_PER_CONNECTION,
};
use turnstile_wire::{
    Create, Delivery, DeviceId, Event, Kind, Maker, Millis, PointerCaps, Reason, Reply, Request,
};

fn all() -> AllowKinds {
    AllowKinds(Kind::ALL.to_vec())
}

fn refused(handled: &Handled) -> Option<Reason> {
    handled.as_ref().err().map(|refused| refused.reason)
}

fn created(handled: &Handled) -> Option<(DeviceId, Delivery)> {
    match handled {
        Ok(Some(Outgoing {
            reply: Reply::Created { device, delivery },
            ..
        })) => Some((*device, *delivery)),
        _ => None,
    }
}

fn events(device: u8, events: Vec<Event>) -> Request {
    Request::Events {
        device: DeviceId(device),
        events,
    }
}

fn wheel(v: i32) -> Event {
    Event::new(EV_REL, REL_WHEEL, v)
}

const T0: Millis = Millis(0);

#[test]
fn a_palmrest_device_is_made_fed_and_destroyed_with_the_connection() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    assert!(s.in_handshake());

    let made = s.handle(T0, Request::Create(Create::wheel()));
    assert_eq!(created(&made), Some((DeviceId(0), Delivery::Proxied)));
    assert!(!s.in_handshake());
    assert_eq!(backend.log().specs[0].name, "palmrest wheel");

    let fed = s.handle(T0, events(0, vec![wheel(-1), Event::SYNC]));
    assert!(matches!(fed, Ok(None)));
    assert_eq!(backend.log().writes, [(0, vec![wheel(-1), Event::SYNC])]);

    drop(s);
    assert_eq!(backend.log().fates, [Fate::Destroyed]);
}

#[test]
fn only_the_events_inside_the_ceiling_reach_the_kernel() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::keys(vec![30, 29])))
        .expect("the set-up step is accepted");
    let typed = Event::new(EV_KEY, 30, 1);
    let ctrl = Event::new(EV_KEY, 29, 1);
    let handled = s.handle(T0, events(0, vec![typed, ctrl, typed, Event::SYNC]));
    assert!(handled.is_ok());
    // The letter before control is held is dropped; the same letter after it goes through.
    assert_eq!(backend.log().writes, [(0, vec![ctrl, typed, Event::SYNC])]);
    assert_eq!(s.stats().denied_events, 1);
}

#[test]
fn a_destroyed_device_is_gone_and_events_for_it_end_the_connection() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::wheel()))
        .expect("the set-up step is accepted");
    let gone = s.handle(
        T0,
        Request::Destroy {
            device: DeviceId(0),
        },
    );
    assert!(matches!(gone, Ok(None)));
    assert_eq!(s.held(), 0);
    assert_eq!(backend.log().fates, [Fate::Destroyed]);
    let after = s.handle(T0, events(0, vec![wheel(1)]));
    assert_eq!(refused(&after), Some(Reason::Unknown));
}

#[test]
fn a_handed_device_leaves_the_service_and_is_not_filtered() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    let made = s.handle(T0, Request::Create(Create::keyboard()));
    assert_eq!(created(&made), Some((DeviceId(0), Delivery::Handed)));
    assert!(matches!(made, Ok(Some(Outgoing { fd: Some(_), .. }))));
    assert_eq!(s.held(), 0, "the client owns it now");
    assert_eq!(backend.log().fates, [Fate::Handed]);
    assert_eq!(backend.log().specs[0].name, "keycap keyboard");
    // Events for a handed device are not the service's to write.
    let stray = s.handle(T0, events(0, vec![wheel(1)]));
    assert_eq!(refused(&stray), Some(Reason::Unknown));
}

#[test]
fn a_pointer_is_cut_to_the_ceiling_before_the_kernel_hears_of_it() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    let caps = PointerCaps {
        label: "Mouse".to_owned(),
        buttons: vec![BTN_LEFT, 30],
        rels: vec![REL_X, REL_Y],
        ..PointerCaps::default()
    };
    let made = s.handle(T0, Request::Create(Create::pointer(caps)));
    assert_eq!(created(&made), Some((DeviceId(0), Delivery::Handed)));
    let spec = backend.log().specs[0].clone();
    assert_eq!(spec.keys, [BTN_LEFT], "key 30 is dropped");
    assert_eq!(spec.name, "Mouse (keycap pointer)");
}

#[test]
fn a_kind_the_caller_may_not_have_is_refused_and_nothing_is_made() {
    let backend = FakeBackend::new();
    let admission = AllowKinds(vec![Kind::PalmrestWheel]);
    let mut s = Session::new(&backend, &admission);
    let handled = s.handle(T0, Request::Create(Create::keyboard()));
    let Err(Refused { reason, note }) = handled else {
        panic!("a refusal is noted for the log");
    };
    assert_eq!(reason, Reason::NotAllowed);
    assert_eq!(note.kind, Some(Kind::KeycapKeyboard));
    assert!(backend.log().specs.is_empty());
}

#[test]
fn requests_outside_the_ceiling_or_the_kernel_are_refused_with_their_reasons() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    let bad = s.handle(T0, Request::Create(Create::keys(vec![9999])));
    assert_eq!(refused(&bad), Some(Reason::BadCaps));
    backend.log().fail_create = Some(std::io::ErrorKind::PermissionDenied);
    let unavailable = s.handle(T0, Request::Create(Create::wheel()));
    assert_eq!(refused(&unavailable), Some(Reason::Unavailable));
}

#[test]
fn a_connection_has_at_most_four_proxied_devices() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    for _ in 0..MAX_DEVICES_PER_CONNECTION {
        let made = s.handle(T0, Request::Create(Create::wheel()));
        assert!(created(&made).is_some());
    }
    let fifth = s.handle(T0, Request::Create(Create::wheel()));
    assert_eq!(refused(&fifth), Some(Reason::TooMany));
    assert_eq!(backend.log().specs.len(), MAX_DEVICES_PER_CONNECTION);
}

#[test]
fn handed_devices_and_creations_are_counted_for_the_life_of_the_connection() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    for _ in 0..MAX_HANDED_PER_CONNECTION {
        assert!(created(&s.handle(T0, Request::Create(Create::keyboard()))).is_some());
    }
    let over = s.handle(T0, Request::Create(Create::keyboard()));
    assert_eq!(refused(&over), Some(Reason::TooMany));

    // Proxied devices come and go, but ids are never reused and the total is capped.
    let backend = FakeBackend::new();
    let mut s = Session::new(&backend, &admission);
    let mut ids = Vec::new();
    for _ in 0..MAX_CREATES_PER_CONNECTION {
        let (id, _) = created(&s.handle(T0, Request::Create(Create::wheel()))).expect("made");
        s.handle(T0, Request::Destroy { device: id })
            .expect("the set-up step is accepted");
        ids.push(id.0);
    }
    assert_eq!(ids, (0..MAX_CREATES_PER_CONNECTION).collect::<Vec<_>>());
    let over = s.handle(T0, Request::Create(Create::wheel()));
    assert_eq!(refused(&over), Some(Reason::TooMany));
}

#[test]
fn events_above_the_rate_are_dropped_counted_and_a_sustained_flood_is_cut() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::wheel()))
        .expect("the set-up step is accepted");
    let one = || events(0, vec![wheel(1), Event::SYNC]);
    // A burst at one instant: the first EVENT_BURST pass, the rest are dropped without ending the connection.
    for _ in 0..100 {
        assert!(s.handle(T0, one()).is_ok());
    }
    assert_eq!(backend.log().writes.len() as u64, EVENT_BURST);
    assert_eq!(s.stats().dropped_datagrams, 100 - EVENT_BURST);
    // Ten times the rate for longer than the cut: the connection ends.
    let mut ended = None;
    for ms in 0..=FLOOD_CUT_MS + 500 {
        let handled = s.handle(Millis(ms), one());
        if let Some(reason) = refused(&handled) {
            ended = Some((ms, reason));
            break;
        }
    }
    let (at, reason) = ended.expect("a flood is cut");
    assert_eq!(reason, Reason::TooMany);
    assert!(at >= FLOOD_CUT_MS, "cut at {at} ms");
}

#[test]
fn a_peer_that_keeps_sending_what_the_ceiling_refuses_is_cut() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::wheel()))
        .expect("the set-up step is accepted");
    let hostile = vec![Event::new(EV_KEY, 30, 1); 60];
    let mut ms = 0;
    let mut ended = None;
    for _ in 0..10 {
        ms += 100;
        let handled = s.handle(Millis(ms), events(0, hostile.clone()));
        if let Some(reason) = refused(&handled) {
            ended = Some(reason);
            break;
        }
    }
    assert_eq!(ended, Some(Reason::NotAllowed));
    assert!(s.stats().denied_events >= MAX_DENIED_EVENTS);
    assert!(
        backend.log().writes.is_empty(),
        "nothing hostile was written"
    );
}

#[test]
fn a_failing_write_ends_the_connection_with_a_reason() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::wheel()))
        .expect("the set-up step is accepted");
    backend.log().fail_write = Some(std::io::ErrorKind::BrokenPipe);
    let handled = s.handle(T0, events(0, vec![wheel(1), Event::SYNC]));
    assert_eq!(refused(&handled), Some(Reason::Unavailable));
}

#[test]
fn devices_of_one_connection_are_independent() {
    let backend = FakeBackend::new();
    let admission = all();
    let mut s = Session::new(&backend, &admission);
    s.handle(T0, Request::Create(Create::touchpad()))
        .expect("the set-up step is accepted");
    s.handle(T0, Request::Create(Create::wheel()))
        .expect("the set-up step is accepted");
    // A wheel event sent to the touchpad is outside the touchpad's ceiling.
    let wrong = s.handle(T0, events(0, vec![wheel(1)]));
    assert!(wrong.is_ok());
    assert!(backend.log().writes.is_empty());
    s.handle(T0, events(1, vec![wheel(1)]))
        .expect("the set-up step is accepted");
    assert_eq!(backend.log().writes, [(1, vec![wheel(1)])]);
}

/// NEW-FA-4: the kernel is told, for every kind, the service the device is made for; the label's
/// role is the peephole role the policy admitted the caller under.
#[test]
fn every_device_kind_is_made_with_the_label_of_the_caller_it_is_for() {
    let pointer = PointerCaps {
        buttons: vec![BTN_LEFT],
        rels: vec![REL_X],
        ..PointerCaps::default()
    };
    let want = [
        (Create::touchpad(), PAD_DAEMON),
        (Create::wheel(), PAD_DAEMON),
        (Create::keys(vec![30, 29]), PAD_DAEMON),
        (Create::keyboard(), KEYMAP_DAEMON),
        (Create::pointer(pointer), KEYMAP_DAEMON),
    ];
    for (create, role) in want {
        let backend = FakeBackend::new();
        let admission = all();
        let mut s = Session::new(&backend, &admission);
        let kind = create.kind();
        let made = s.handle(T0, Request::Create(create));
        assert!(created(&made).is_some(), "{kind:?}");
        let label = backend.log().specs[0].maker.to_string();
        assert_eq!(label, format!("turnstile/{}", role.as_str()), "{kind:?}");
        assert_eq!(Maker::parse(&label), Some(kind.maker()), "{kind:?}");
    }
}
