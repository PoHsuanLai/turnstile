//! The accept loop on a real socket in a scratch folder.

use crate::support::{USER, listed, palmrest};
use rustix::net::{
    AddressFamily, SocketAddrUnix, SocketFlags, SocketType, bind, listen, socket_with,
};
use std::collections::VecDeque;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::{Mutex, mpsc};
use std::time::Duration;
use turnstile::backend::fake::FakeBackend;
use turnstile::clock::Monotonic;
use turnstile::daemon::{Daemon, Settings};
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::identity::{ActiveUid, Authority, Caller, Credential, IdentifyError};
use turnstile::serve::Context;
use turnstile_client::{Broker, ClientError, Create, DeviceSource, Made, Reason, Session};
use turnstile_wire::limits::{MAX_CONNECTIONS_PER_USER, MAX_RESERVED_PER_CLIENT};

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
    start_with(idle_ms, FixedAuthority::naming(palmrest()))
}

/// Names the next peers in the order given, then `rest` for every one after.
struct Scripted {
    next: Mutex<VecDeque<Caller>>,
    rest: Caller,
}

impl Authority for Scripted {
    fn identify(&self, _conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        let mut next = self.next.lock().expect("lock");
        Ok(next.pop_front().unwrap_or_else(|| self.rest.clone()))
    }
}

/// A user the service has never heard of, at the seat or not.
fn stranger(uid: u32) -> Caller {
    Caller {
        credential: Credential::Unlisted,
        ..listed(
            "pad_daemon",
            turnstile::identity::Strength::Cgroup,
            uid,
            "/x",
        )
    }
}

fn start_with<A: Authority>(
    idle_ms: u64,
    authority: A,
) -> (tempfile::TempDir, Broker, mpsc::Receiver<()>, FakeBackend) {
    let dir = tempfile::tempdir().expect("scratch folder");
    let path = dir.path().join("input.sock");
    let kernel = FakeBackend::new();
    let context = Context {
        backend: kernel.clone(),
        authority,
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

/// NEW-PI-6: one user's idle connections fill that user's slots and nothing else's.
#[test]
fn a_flood_from_one_user_is_refused_after_its_own_slots_and_palmrest_still_gets_in() {
    let script: VecDeque<Caller> = (0..=MAX_CONNECTIONS_PER_USER)
        .map(|_| stranger(2000))
        .chain([stranger(2001)])
        .collect();
    let authority = Scripted {
        next: Mutex::new(script),
        rest: palmrest(),
    };
    let (_dir, broker, _finished, _kernel) = start_with(60_000, authority);
    let held: Vec<_> = (0..MAX_CONNECTIONS_PER_USER)
        .map(|_| broker.open().expect("connects"))
        .collect();
    let mut extra = broker.open().expect("the kernel queues it");
    let err = extra
        .create(&Create::wheel())
        .expect_err("over the user's slots");
    assert!(
        matches!(
            err,
            ClientError::Refused(Reason::TooMany) | ClientError::Closed
        ),
        "{err:?}"
    );
    // Another user is not affected: it is served, and refused for what it is, not for the crowd.
    let mut other = broker.open().expect("connects");
    let err = other
        .create(&Create::wheel())
        .expect_err("a stranger has no device");
    assert!(
        !matches!(err, ClientError::Refused(Reason::TooMany)),
        "{err:?}"
    );
    // And palmrest, proven, has its own reserve.
    let mut pad = broker.open().expect("connects");
    assert!(matches!(pad.create(&Create::wheel()), Ok(Made::Proxied(_))));
    drop(held);
}

/// NEW-PI-6: palmrest's reserve is its own and has a cap too.
#[test]
fn proven_clients_have_their_own_cap() {
    let (_dir, broker, _finished, _kernel) = start(60_000);
    let held: Vec<_> = (0..MAX_RESERVED_PER_CLIENT)
        .map(|_| broker.open().expect("connects"))
        .collect();
    let mut extra = broker.open().expect("the kernel queues it");
    let err = extra
        .create(&Create::wheel())
        .expect_err("over the reserve");
    assert!(
        matches!(
            err,
            ClientError::Refused(Reason::TooMany) | ClientError::Closed
        ),
        "{err:?}"
    );
    drop(held);
}
