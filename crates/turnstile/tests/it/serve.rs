//! The whole connection, over a socket pair, with the real client on one end and the fake kernel
//! and a fixed authority on the other.

use crate::support::{USER, context, keycap, palmrest};
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, recv, send, socketpair,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use turnstile::backend::fake::Fate;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::identity::{ActiveUid, Caller};
use turnstile::serve::{Context, serve};
use turnstile_client::{
    ClientError, Connection, Create, DeviceId, Event, Made, Reason, Sent, Session,
};
use turnstile_wire::codes::{EV_REL, REL_WHEEL};
use turnstile_wire::limits::MAX_DATAGRAM;
use turnstile_wire::{Reply, decode_reply};

type Ctx = crate::support::TestContext;

/// Starts a connection thread on one end of a pair and returns the client on the other, plus the
/// thread, which ends when the client goes.
fn connect(ctx: &Arc<Ctx>) -> (Connection, std::thread::JoinHandle<()>) {
    let (client, service) = socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket pair");
    // Both ends block: the client sends with DONTWAIT and waits with poll whatever the mode, and the
    // service end must wait in recv as the accepted socket of the real service does.
    let ctx = Arc::clone(ctx);
    let thread = std::thread::spawn(move || serve(service, &ctx));
    (
        Connection::over_with_timeout(client, Duration::from_secs(5)),
        thread,
    )
}

fn wheel() -> Vec<Event> {
    vec![Event::new(EV_REL, REL_WHEEL, 1), Event::SYNC]
}

#[test]
fn palmrest_gets_a_proxied_wheel_and_loses_it_when_it_hangs_up() {
    let (ctx, kernel) = context(palmrest());
    let (mut client, thread) = connect(&ctx);
    let Made::Proxied(id) = client.create(&Create::wheel()).expect("created") else {
        panic!("a wheel is proxied");
    };
    assert_eq!(client.send(id, &wheel()).expect("sent"), Sent::Queued);
    drop(client);
    thread
        .join()
        .expect("the connection ends when the client goes");
    let log = kernel.log();
    assert_eq!(log.specs[0].name, "palmrest wheel");
    assert_eq!(log.writes, [(0, wheel())]);
    assert_eq!(
        log.fates,
        [Fate::Destroyed],
        "devices die with their owner connection"
    );
}

#[test]
fn keycap_gets_its_keyboard_as_a_descriptor_that_outlives_the_connection() {
    let (ctx, kernel) = context(keycap());
    let (mut client, thread) = connect(&ctx);
    let Made::Handed { fd, .. } = client.create(&Create::keyboard()).expect("created") else {
        panic!("a keyboard is handed");
    };
    drop(client);
    thread.join().expect("ended");
    assert_eq!(
        kernel.log().fates,
        [Fate::Handed],
        "the service does not destroy a handed device"
    );
    // The descriptor still reaches the kernel's end.
    send(&fd, b"k", SendFlags::NOSIGNAL).expect("write to the device");
    let mut buf = [0u8; 4];
    let log = kernel.log();
    let (n, _) = recv(&log.handed_peers[0], &mut buf[..], RecvFlags::empty()).expect("read");
    assert_eq!(&buf[..n], b"k");
}

#[test]
fn a_caller_asking_for_another_programs_device_is_refused_and_cut() {
    let (ctx, kernel) = context(palmrest());
    let (mut client, thread) = connect(&ctx);
    let err = client.create(&Create::keyboard()).expect_err("refused");
    assert!(
        matches!(err, ClientError::Refused(Reason::NotAllowed)),
        "{err:?}"
    );
    thread.join().expect("the service closed the connection");
    assert!(kernel.log().specs.is_empty());
}

#[test]
fn a_palmrest_running_as_the_person_at_the_seat_is_refused() {
    let (ctx, _) = context(Caller {
        uid: USER,
        ..palmrest()
    });
    let (mut client, thread) = connect(&ctx);
    let err = client.create(&Create::wheel()).expect_err("refused");
    assert!(
        matches!(err, ClientError::Refused(Reason::NotAllowed)),
        "{err:?}"
    );
    thread.join().expect("ended");
}

#[test]
fn the_seat_is_read_at_each_request_not_once() {
    // keycap before anyone logs in, on a seat with no active user.
    let kernel = turnstile::backend::fake::FakeBackend::new();
    let ctx = Arc::new(Context {
        backend: kernel.clone(),
        authority: FixedAuthority::naming(keycap()),
        seat: FixedSeat(ActiveUid(None)),
        clock: turnstile::clock::ManualClock::new(),
    });
    let (mut client, thread) = connect(&ctx);
    assert!(matches!(
        client.create(&Create::keyboard()),
        Ok(Made::Handed { .. })
    ));
    drop(client);
    thread.join().expect("ended");
}

#[test]
fn a_peer_that_cannot_be_identified_is_refused_before_anything_else() {
    let kernel = turnstile::backend::fake::FakeBackend::new();
    let ctx = Arc::new(Context {
        backend: kernel.clone(),
        authority: FixedAuthority::failing(),
        seat: FixedSeat(ActiveUid(Some(USER))),
        clock: turnstile::clock::ManualClock::new(),
    });
    let (mut client, thread) = connect(&ctx);
    // Either the refusal arrives, or the connection is already shut: never a device.
    let err = client.create(&Create::wheel()).expect_err("no device");
    assert!(
        matches!(
            err,
            ClientError::Refused(Reason::Unknown) | ClientError::Closed
        ),
        "{err:?}"
    );
    thread.join().expect("ended");
    assert!(kernel.log().specs.is_empty());
}

/// A raw socket pair for the tests that send what the client never would.
fn raw(ctx: &Arc<Ctx>) -> (rustix::fd::OwnedFd, std::thread::JoinHandle<()>) {
    let (peer, service) = socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket pair");
    let ctx = Arc::clone(ctx);
    (peer, std::thread::spawn(move || serve(service, &ctx)))
}

fn next_reply(peer: &rustix::fd::OwnedFd) -> Reply {
    let mut buf = [0u8; 64];
    let (n, _) = recv(peer, &mut buf[..], RecvFlags::empty()).expect("a reply");
    decode_reply(&buf[..n]).expect("parses")
}

#[test]
fn garbage_and_overlong_datagrams_end_the_connection_with_unknown() {
    let (ctx, _) = context(palmrest());
    for datagram in [vec![0xff; 5], vec![1, 77], vec![0u8; MAX_DATAGRAM + 1]] {
        let (peer, thread) = raw(&ctx);
        send(&peer, &datagram, SendFlags::NOSIGNAL).expect("send");
        assert_eq!(
            next_reply(&peer),
            Reply::Refused(Reason::Unknown),
            "{} bytes",
            datagram.len()
        );
        thread.join().expect("ended");
    }
}

#[test]
fn a_connection_that_asks_for_nothing_is_closed_after_the_handshake_timeout() {
    let (ctx, _) = context(palmrest());
    let (peer, thread) = raw(&ctx);
    let started = Instant::now();
    assert_eq!(next_reply(&peer), Reply::Refused(Reason::Unknown));
    thread.join().expect("ended");
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(900) && waited < Duration::from_secs(5),
        "{waited:?}"
    );
}

#[test]
fn a_connection_with_a_device_may_stay_silent() {
    let (ctx, _) = context(palmrest());
    let (mut client, thread) = connect(&ctx);
    assert!(client.create(&Create::wheel()).is_ok());
    std::thread::sleep(Duration::from_millis(1_300));
    // Still there after longer than the handshake timeout.
    assert_eq!(
        client.send(DeviceId(0), &wheel()).expect("sent"),
        Sent::Queued
    );
    drop(client);
    thread.join().expect("ended");
}
