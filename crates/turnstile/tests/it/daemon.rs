//! The accept loop on a real socket in a scratch folder.

use crate::support::{USER, listed, palmrest};
use peephole_core::Uid;
use rustix::net::{
    AddressFamily, SocketAddrUnix, SocketFlags, SocketType, bind, listen, socket_with,
};
use std::collections::{HashMap, VecDeque};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::{Condvar, Mutex, mpsc};
use std::time::Duration;
use turnstile::backend::fake::FakeBackend;
use turnstile::clock::Monotonic;
use turnstile::daemon::{Daemon, Settings};
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::seat::FixedSeat;
use turnstile::identity::{ActiveUid, Authority, Caller, IdentifyError};
use turnstile::pending::Caps;
use turnstile::serve::Context;
use turnstile_client::{Broker, ClientError, Create, DeviceSource, Made, Reason, Session};
use turnstile_wire::limits::{
    MAX_CONNECTIONS_PER_USER, MAX_PENDING_PER_USER, MAX_RESERVED_PER_CLIENT,
};

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

/// Room for any number of identifications at once: the slot tests are about slots, and a burst of
/// connections in a test must not meet the pending caps (those have tests of their own).
const WIDE: Caps = Caps {
    per_user: 64,
    total: 64,
};

/// How long a connection waits, once it is next in line, before it is identified: long enough for
/// the one before it to have taken its slot.
const SETTLE: Duration = Duration::from_millis(10);

#[derive(Default)]
struct Turn {
    /// Connections accepted so far; the next one is numbered with this.
    accepted: u64,
    /// The connection that may be identified now.
    now: u64,
    by_fd: HashMap<RawFd, u64>,
}

/// Identifies connections one at a time in the order they were accepted, as the single accept
/// thread once did. Since identification moved to a thread per connection (NEW-FA-3), threads race
/// for slots; the slot tests count slots in connection order ("the fifth is refused"), so they need
/// the order back. Every accepted connection must reach `identify`, so it is not used where the
/// daemon closes connections at accept (a pending cap) or where one identification hangs.
struct InOrder<A> {
    inner: A,
    turn: Mutex<Turn>,
    changed: Condvar,
}

impl<A: Authority> InOrder<A> {
    fn new(inner: A) -> Self {
        Self {
            inner,
            turn: Mutex::default(),
            changed: Condvar::new(),
        }
    }
}

impl<A: Authority> Authority for InOrder<A> {
    fn peer_uid(&self, conn: &OwnedFd) -> Result<Uid, IdentifyError> {
        let uid = self.inner.peer_uid(conn)?;
        let mut turn = self.turn.lock().expect("lock");
        let number = turn.accepted;
        turn.accepted += 1;
        turn.by_fd.insert(conn.as_raw_fd(), number);
        Ok(uid)
    }

    fn identify(&self, conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        let number = {
            let turn = self.turn.lock().expect("lock");
            let number = turn.by_fd.get(&conn.as_raw_fd()).copied().unwrap_or(0);
            let _turn = self
                .changed
                .wait_while(turn, |turn| turn.now != number)
                .expect("lock");
            number
        };
        if number > 0 {
            std::thread::sleep(SETTLE);
        }
        let named = self.inner.identify(conn);
        self.turn.lock().expect("lock").now = number + 1;
        self.changed.notify_all();
        named
    }
}

/// Runs a daemon on a fresh socket; the receiver says when `run` returned.
fn start(idle_ms: u64) -> (tempfile::TempDir, Broker, mpsc::Receiver<()>, FakeBackend) {
    start_with(idle_ms, FixedAuthority::naming(palmrest()))
}

/// Names the next peers in the order they are accepted, then `rest` for every one after. The peer
/// is chosen when the daemon asks for its user (on the accept loop, in order), so the answer does
/// not depend on which connection's thread runs first. A peer of a `hung` user is never identified
/// until the gate opens (the sender is dropped).
struct Scripted {
    next: Mutex<VecDeque<Caller>>,
    rest: Caller,
    by_fd: Mutex<HashMap<RawFd, Caller>>,
    hung: Option<(Uid, Mutex<mpsc::Receiver<()>>)>,
}

impl Scripted {
    fn new(next: VecDeque<Caller>, rest: Caller) -> Self {
        Self {
            next: Mutex::new(next),
            rest,
            by_fd: Mutex::default(),
            hung: None,
        }
    }

    /// Identification of peers of `uid` blocks until the returned sender is dropped.
    fn hanging(mut self, uid: Uid) -> (Self, mpsc::Sender<()>) {
        let (gate, wait) = mpsc::channel();
        self.hung = Some((uid, Mutex::new(wait)));
        (self, gate)
    }
}

impl Authority for Scripted {
    fn peer_uid(&self, conn: &OwnedFd) -> Result<Uid, IdentifyError> {
        let caller = self
            .next
            .lock()
            .expect("lock")
            .pop_front()
            .unwrap_or_else(|| self.rest.clone());
        let uid = caller.uid;
        self.by_fd
            .lock()
            .expect("lock")
            .insert(conn.as_raw_fd(), caller);
        Ok(uid)
    }

    fn identify(&self, conn: &OwnedFd) -> Result<Caller, IdentifyError> {
        let caller = self
            .by_fd
            .lock()
            .expect("lock")
            .get(&conn.as_raw_fd())
            .cloned()
            .unwrap_or_else(|| self.rest.clone());
        if let Some((uid, gate)) = &self.hung
            && *uid == caller.uid
        {
            // Ends with an error when the sender drops: that is the gate opening.
            let _ = gate.lock().expect("lock").recv();
        }
        Ok(caller)
    }
}

/// A user the service has never heard of, at the seat or not.
fn stranger(uid: Uid) -> Caller {
    Caller {
        credential: None,
        ..listed("pad_daemon", peephole_core::Proof::Cgroup, uid, "/x")
    }
}

fn start_with<A: Authority>(
    idle_ms: u64,
    authority: A,
) -> (tempfile::TempDir, Broker, mpsc::Receiver<()>, FakeBackend) {
    start_capped(idle_ms, InOrder::new(authority), WIDE)
}

fn start_capped<A: Authority>(
    idle_ms: u64,
    authority: A,
    pending: Caps,
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
        pending,
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
        .map(|_| stranger(Uid::new(2000)))
        .chain([stranger(Uid::new(2001))])
        .collect();
    let authority = Scripted::new(script, palmrest());
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

/// Runs `work` on a thread and fails if it takes more than five seconds: a stalled accept loop
/// would otherwise hang the test for ever.
fn promptly<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    result
        .recv_timeout(Duration::from_secs(5))
        .expect("finished in time")
}

/// NEW-FA-3: a peer whose identification never returns holds its own thread and nobody else's.
#[test]
fn a_stuck_identification_does_not_delay_palmrest() {
    let script = VecDeque::from([stranger(Uid::new(3000))]);
    let (authority, release) = Scripted::new(script, palmrest()).hanging(Uid::new(3000));
    let (_dir, broker, _finished, _kernel) = start_capped(60_000, authority, Caps::default());
    let _stuck = broker.open().expect("connects");
    let served = promptly(move || {
        let mut pad = broker.open().expect("connects");
        matches!(pad.create(&Create::wheel()), Ok(Made::Proxied(_)))
    });
    assert!(served, "palmrest waited behind a stuck identification");
    drop(release);
}

/// NEW-FA-3: one user's stuck identifications use up that user's share and no one else's.
#[test]
fn a_user_over_its_pending_share_is_closed_at_once_and_palmrest_still_gets_in() {
    let script: VecDeque<Caller> = (0..=MAX_PENDING_PER_USER)
        .map(|_| stranger(Uid::new(3000)))
        .collect();
    let (authority, release) = Scripted::new(script, palmrest()).hanging(Uid::new(3000));
    let (_dir, broker, _finished, _kernel) = start_capped(60_000, authority, Caps::default());
    let _stuck: Vec<_> = (0..MAX_PENDING_PER_USER)
        .map(|_| broker.open().expect("connects"))
        .collect();
    let (refused, served) = promptly(move || {
        let mut extra = broker.open().expect("connects");
        let refused = extra.create(&Create::wheel());
        let mut pad = broker.open().expect("connects");
        let served = pad.create(&Create::wheel());
        (refused, served)
    });
    assert!(
        matches!(
            refused,
            Err(ClientError::Refused(Reason::TooMany) | ClientError::Closed)
        ),
        "{refused:?}"
    );
    assert!(matches!(served, Ok(Made::Proxied(_))), "{served:?}");
    drop(release);
}
