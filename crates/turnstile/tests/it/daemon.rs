//! The accept loop on a real socket in a scratch folder.

use crate::support::{USER, palmrest};
use rustix::net::{
    AddressFamily, SocketAddrUnix, SocketFlags, SocketType, bind, listen, socket_with,
};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use turnstile::backend::fake::FakeBackend;
use turnstile::clock::Monotonic;
use turnstile::daemon::{Daemon, Settings};
use turnstile::identity::ActiveUid;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::serve::Context;
use turnstile_client::{Broker, ClientError, Create, DeviceSource, Made, Reason, Session};
use turnstile_wire::limits::MAX_CONNECTIONS;

fn listener(path: &Path) -> OwnedFd {
    let fd = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket");
    bind(&fd, &SocketAddrUnix::new(path).expect("address")).expect("bind");
    listen(&fd, 32).expect("listen");
    fd
}

/// Runs a daemon on a fresh socket; the receiver says when `run` returned.
fn start(idle_ms: u64) -> (tempfile::TempDir, Broker, mpsc::Receiver<()>, FakeBackend) {
    let dir = tempfile::tempdir().expect("scratch folder");
    let path = dir.path().join("input.sock");
    let kernel = FakeBackend::new();
    let context = Context {
        backend: kernel.clone(),
        authority: FixedAuthority::naming(palmrest(USER)),
        seat: FixedSeat(ActiveUid(Some(USER))),
        clock: Monotonic::new(),
    };
    let settings = Settings {
        idle: Duration::from_millis(idle_ms),
        slice: Duration::from_millis(20),
    };
    let daemon = Daemon::new(listener(&path), context, settings);
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let result = daemon.run();
        let _ = done.send(());
        result
    });
    (dir, Broker::at(path), finished, kernel)
}

#[test]
fn the_service_exits_by_itself_when_no_one_is_connected() {
    let (_dir, _broker, finished, _kernel) = start(200);
    finished
        .recv_timeout(Duration::from_secs(5))
        .expect("an idle service exits");
}

#[test]
fn a_connection_keeps_it_alive_and_its_end_lets_it_go() {
    let (_dir, broker, finished, kernel) = start(600);
    let mut session = broker.open().expect("connects");
    assert!(matches!(
        session.create(&Create::wheel()),
        Ok(Made::Proxied(_))
    ));
    assert!(
        finished.recv_timeout(Duration::from_millis(1_500)).is_err(),
        "exited while a connection was open"
    );
    drop(session);
    finished
        .recv_timeout(Duration::from_secs(5))
        .expect("exits once the last connection is gone");
    assert_eq!(kernel.log().specs.len(), 1);
}

#[test]
fn connections_over_the_cap_are_refused_with_too_many() {
    let (_dir, broker, finished, _kernel) = start(60_000);
    let held: Vec<_> = (0..MAX_CONNECTIONS)
        .map(|_| broker.open().expect("connects"))
        .collect();
    // Give the accept loop time to take all of them before the next one arrives.
    std::thread::sleep(Duration::from_millis(300));
    let mut extra = broker.open().expect("the kernel queues it");
    let err = extra.create(&Create::wheel()).expect_err("over the cap");
    assert!(
        matches!(
            err,
            ClientError::Refused(Reason::TooMany) | ClientError::Closed
        ),
        "{err:?}"
    );
    drop(held);
    drop(finished);
}
