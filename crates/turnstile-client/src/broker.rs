//! The real service: a `SOCK_SEQPACKET` connection to turnstile's socket.

use crate::error::ClientError;
use crate::link::Link;
use crate::session::{DeviceSource, Made, Sent, Session};
use rustix::io::Errno;
use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType, connect, socket_with};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use turnstile_wire::limits::MAX_EVENTS_PER_DATAGRAM;
use turnstile_wire::{
    Create, Delivery, DeviceId, Event, Reason, Reply, Request, decode_reply, encode_request,
};

/// Where the service's socket unit puts its socket.
pub const DEFAULT_SOCKET: &str = "/run/turnstile/input.sock";

/// How long a `Create` may wait for its answer: the service makes a device in well under a
/// millisecond, so a second means it is stuck.
const CREATE_TIMEOUT: Duration = Duration::from_secs(2);

/// Connects to the service's socket.
#[derive(Debug, Clone)]
pub struct Broker {
    path: PathBuf,
}

impl Default for Broker {
    fn default() -> Self {
        Self::at(DEFAULT_SOCKET)
    }
}

impl Broker {
    /// A broker for the socket at `path`.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl DeviceSource for Broker {
    type Session = Connection;

    fn open(&self) -> Result<Connection, ClientError> {
        // Close-on-exec so a spawned child never keeps the connection alive; non-blocking so a
        // full queue is an answer, not a wait.
        let fd = socket_with(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            None,
        )
        .map_err(|e| ClientError::Io(e.into()))?;
        let addr =
            SocketAddrUnix::new(self.path.as_path()).map_err(|e| ClientError::Io(e.into()))?;
        match connect(&fd, &addr) {
            Ok(()) => Ok(Connection::over(fd)),
            Err(Errno::NOENT | Errno::CONNREFUSED | Errno::NOTDIR) => Err(ClientError::Absent),
            Err(Errno::AGAIN) => Err(ClientError::Busy),
            Err(e) => Err(ClientError::Io(e.into())),
        }
    }
}

/// One connection. Dropping it closes the socket, which destroys every proxied device.
#[derive(Debug)]
pub struct Connection {
    link: Link,
    create_timeout: Duration,
}

impl Connection {
    /// A connection over `fd`, a non-blocking `SOCK_SEQPACKET` socket already connected to the
    /// service (or to a test's stand-in for it).
    pub fn over(fd: OwnedFd) -> Self {
        Self::over_with_timeout(fd, CREATE_TIMEOUT)
    }

    /// As [`Connection::over`], with `create_timeout` as the longest a `Create` waits for its answer.
    pub fn over_with_timeout(fd: OwnedFd, create_timeout: Duration) -> Self {
        Self {
            link: Link::new(fd),
            create_timeout,
        }
    }
}

impl Session for Connection {
    fn create(&mut self, request: &Create) -> Result<Made, ClientError> {
        let bytes = encode_request(&Request::Create(request.clone()))?;
        match self.link.send(&bytes)? {
            Sent::Queued => {}
            Sent::Dropped => return Err(ClientError::Busy),
        }
        let deadline = Instant::now() + self.create_timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let got = self.link.recv(left)?;
            match decode_reply(&got.bytes)? {
                Reply::Created {
                    device,
                    delivery: Delivery::Proxied,
                } => return Ok(Made::Proxied(device)),
                Reply::Created {
                    device,
                    delivery: Delivery::Handed,
                } => {
                    return got
                        .fd
                        .map(|fd| Made::Handed { device, fd })
                        .ok_or(ClientError::NoDevice);
                }
                Reply::Refused(reason) => return Err(ClientError::Refused(reason)),
                // Lights written to an earlier device may arrive first; they are not the answer.
                Reply::Feedback { .. } => {}
                _ => return Err(ClientError::Unexpected),
            }
        }
    }

    fn send(&mut self, device: DeviceId, events: &[Event]) -> Result<Sent, ClientError> {
        for chunk in events.chunks(MAX_EVENTS_PER_DATAGRAM) {
            let bytes = encode_request(&Request::Events {
                device,
                events: chunk.to_vec(),
            })?;
            match self.link.send(&bytes)? {
                Sent::Queued => {}
                // The rest of the frame would arrive without its start: stop here.
                Sent::Dropped => return Ok(Sent::Dropped),
            }
        }
        Ok(Sent::Queued)
    }

    fn destroy(&mut self, device: DeviceId) -> Result<(), ClientError> {
        let bytes = encode_request(&Request::Destroy { device })?;
        match self.link.send(&bytes)? {
            Sent::Queued => Ok(()),
            Sent::Dropped => Err(ClientError::Busy),
        }
    }

    fn refusal(&mut self) -> Option<Reason> {
        loop {
            let got = self.link.try_recv().ok().flatten()?;
            if let Ok(Reply::Refused(reason)) = decode_reply(&got.bytes) {
                return Some(reason);
            }
        }
    }
}
