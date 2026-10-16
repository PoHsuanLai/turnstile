use crate::stand::{Stand, pair};
use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
use std::time::Duration;
use turnstile_client::{
    Broker, ClientError, Connection, Create, DeviceId, DeviceSource, Event, Made, Reason, Sent,
    Session,
};
use turnstile_wire::codes::{EV_REL, REL_WHEEL};
use turnstile_wire::limits::MAX_EVENTS_PER_DATAGRAM;
use turnstile_wire::{Delivery, Reply, Request};

fn connection(fd: std::os::fd::OwnedFd) -> Connection {
    Connection::over_with_timeout(fd, Duration::from_millis(150))
}

fn wheel_events(n: usize) -> Vec<Event> {
    vec![Event::new(EV_REL, REL_WHEEL, 1); n]
}

#[test]
fn a_proxied_device_is_asked_for_fed_and_destroyed() {
    let (client, stand) = pair();
    let service = std::thread::spawn(move || {
        let mut seen = Vec::new();
        while let Some(request) = stand.next() {
            if matches!(request, Request::Create(_)) {
                stand.reply(&Reply::Created {
                    device: DeviceId(4),
                    delivery: Delivery::Proxied,
                });
            }
            seen.push(request);
        }
        seen
    });
    let mut session = connection(client);
    let made = session.create(&Create::wheel()).expect("created");
    let Made::Proxied(id) = made else {
        panic!("a wheel is proxied, got {made:?}");
    };
    assert_eq!(id, DeviceId(4));
    assert_eq!(
        session.send(id, &wheel_events(3)).expect("sent"),
        Sent::Queued
    );
    session.destroy(id).expect("destroyed");
    drop(session);
    let seen = service.join().expect("service thread");
    assert_eq!(
        seen,
        [
            Request::Create(Create::wheel()),
            Request::Events {
                device: DeviceId(4),
                events: wheel_events(3)
            },
            Request::Destroy {
                device: DeviceId(4)
            },
        ]
    );
}

#[test]
fn more_than_one_datagram_of_events_is_split() {
    let (client, stand) = pair();
    let service = std::thread::spawn(move || {
        let mut sizes = Vec::new();
        while let Some(Request::Events { events, .. }) = stand.next() {
            sizes.push(events.len());
        }
        sizes
    });
    let mut session = connection(client);
    let n = 2 * MAX_EVENTS_PER_DATAGRAM + 2;
    assert_eq!(
        session.send(DeviceId(0), &wheel_events(n)).expect("sent"),
        Sent::Queued
    );
    drop(session);
    assert_eq!(service.join().expect("service"), [64, 64, 2]);
}

#[test]
fn a_handed_device_arrives_as_a_descriptor() {
    let (client, stand) = pair();
    let (ours, theirs) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("stand-in device");
    let service = std::thread::spawn(move || {
        assert!(matches!(stand.next(), Some(Request::Create(_))));
        stand.reply_with_fd(
            &Reply::Created {
                device: DeviceId(1),
                delivery: Delivery::Handed,
            },
            &ours,
        );
        stand
    });
    let mut session = connection(client);
    let Made::Handed { device, fd } = session.create(&Create::keyboard()).expect("created") else {
        panic!("a keyboard is handed");
    };
    assert_eq!(device, DeviceId(1));
    rustix::net::send(&fd, b"hello", rustix::net::SendFlags::NOSIGNAL)
        .expect("write to the device");
    let mut buf = [0u8; 16];
    let (n, _) =
        rustix::net::recv(&theirs, &mut buf[..], rustix::net::RecvFlags::empty()).expect("read");
    assert_eq!(&buf[..n], b"hello");
    drop(service.join().expect("service"));
}

#[test]
fn a_handed_reply_without_its_descriptor_is_an_error() {
    let (client, stand) = pair();
    let service = std::thread::spawn(move || {
        stand.next();
        stand.reply(&Reply::Created {
            device: DeviceId(1),
            delivery: Delivery::Handed,
        });
        stand
    });
    let mut session = connection(client);
    let err = session
        .create(&Create::keyboard())
        .expect_err("no descriptor");
    assert!(matches!(err, ClientError::NoDevice), "{err:?}");
    drop(service.join().expect("service"));
}

#[test]
fn a_refusal_is_an_error_naming_the_reason() {
    let (client, stand) = pair();
    let service = std::thread::spawn(move || {
        stand.next();
        stand.reply(&Reply::Refused(Reason::NotAllowed));
        stand
    });
    let mut session = connection(client);
    let err = session.create(&Create::touchpad()).expect_err("refused");
    assert!(
        matches!(err, ClientError::Refused(Reason::NotAllowed)),
        "{err:?}"
    );
    drop(service.join().expect("service"));
}

#[test]
fn feedback_before_the_answer_is_skipped() {
    let (client, stand) = pair();
    let service = std::thread::spawn(move || {
        stand.next();
        stand.reply(&Reply::Feedback {
            device: DeviceId(9),
            events: vec![Event::new(0x11, 1, 1)],
        });
        stand.reply(&Reply::Created {
            device: DeviceId(2),
            delivery: Delivery::Proxied,
        });
        stand
    });
    let mut session = connection(client);
    assert!(matches!(
        session.create(&Create::wheel()),
        Ok(Made::Proxied(DeviceId(2)))
    ));
    drop(service.join().expect("service"));
}

#[test]
fn a_silent_service_is_a_timeout_not_a_hang() {
    let (client, stand) = pair();
    let mut session = connection(client);
    let err = session.create(&Create::wheel()).expect_err("no answer");
    assert!(matches!(err, ClientError::Timeout), "{err:?}");
    drop(stand);
}

#[test]
fn a_service_that_is_behind_drops_frames_and_never_blocks() {
    let (client, stand) = pair();
    let mut session = connection(client);
    let frame = wheel_events(MAX_EVENTS_PER_DATAGRAM);
    let dropped = (0..200_000)
        .map(|_| session.send(DeviceId(0), &frame).expect("send"))
        .any(|sent| sent == Sent::Dropped);
    assert!(dropped, "a queue nobody reads must fill up");
    drop(stand);
}

#[test]
fn a_gone_service_is_closed_not_a_signal() {
    let (client, stand) = pair();
    let mut session = connection(client);
    drop(stand);
    let err = session
        .send(DeviceId(0), &wheel_events(1))
        .expect_err("closed");
    assert!(matches!(err, ClientError::Closed), "{err:?}");
}

#[test]
fn the_reason_for_a_cut_connection_can_be_read_afterwards() {
    let (client, stand) = pair();
    stand.reply(&Reply::Refused(Reason::TooMany));
    let mut session = connection(client);
    assert_eq!(session.refusal(), Some(Reason::TooMany));
    assert_eq!(session.refusal(), None);
    drop(stand);
}

#[test]
fn no_socket_means_absent_and_a_listening_one_connects() {
    let dir = tempfile::tempdir().expect("scratch folder");
    let path = dir.path().join("input.sock");
    let err = Broker::at(&path).open().expect_err("nothing listens yet");
    assert!(matches!(err, ClientError::Absent), "{err:?}");

    let listener = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("listener");
    rustix::net::bind(
        &listener,
        &SocketAddrUnix::new(path.as_path()).expect("address"),
    )
    .expect("bind");
    rustix::net::listen(&listener, 4).expect("listen");
    let service = std::thread::spawn(move || {
        let conn = rustix::net::accept(&listener).expect("accept");
        let stand = Stand { sock: conn };
        stand.next();
        stand.reply(&Reply::Created {
            device: DeviceId(0),
            delivery: Delivery::Proxied,
        });
        stand
    });
    let mut session = Broker::at(&path).open().expect("connects");
    assert!(matches!(
        session.create(&Create::touchpad()),
        Ok(Made::Proxied(_))
    ));
    drop(service.join().expect("service"));
}
