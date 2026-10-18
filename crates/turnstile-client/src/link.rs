//! One `SOCK_SEQPACKET` socket: datagrams out without blocking, datagrams in with a deadline, and
//! the descriptor that may ride with a reply.

use crate::error::ClientError;
use crate::session::Sent;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendFlags, recvmsg, send,
};
use std::io::IoSliceMut;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};
use turnstile_wire::WireError;
use turnstile_wire::limits::MAX_DATAGRAM;

/// A datagram and the descriptor that came with it.
#[derive(Debug)]
pub(crate) struct Received {
    pub bytes: Vec<u8>,
    pub fd: Option<OwnedFd>,
}

#[derive(Debug)]
pub(crate) struct Link {
    fd: OwnedFd,
}

impl Link {
    /// `fd` must be a non-blocking `SOCK_SEQPACKET` socket.
    pub(crate) fn new(fd: OwnedFd) -> Self {
        Self { fd }
    }

    /// Sends one datagram; a full queue is [`Sent::Dropped`], a gone peer is `Closed`.
    pub(crate) fn send(&self, bytes: &[u8]) -> Result<Sent, ClientError> {
        match send(&self.fd, bytes, SendFlags::DONTWAIT | SendFlags::NOSIGNAL) {
            Ok(_) => Ok(Sent::Queued),
            Err(Errno::AGAIN) => Ok(Sent::Dropped),
            Err(Errno::PIPE | Errno::CONNRESET | Errno::NOTCONN) => Err(ClientError::Closed),
            Err(e) => Err(ClientError::Io(e.into())),
        }
    }

    /// Waits up to `timeout` for one datagram.
    pub(crate) fn recv(&self, timeout: Duration) -> Result<Received, ClientError> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if !self.readable(left)? {
                return Err(ClientError::Timeout);
            }
            if let Some(got) = self.read()? {
                return Ok(got);
            }
        }
    }

    /// A datagram that is already there, or `None`. Never blocks.
    pub(crate) fn try_recv(&self) -> Result<Option<Received>, ClientError> {
        self.read()
    }

    fn readable(&self, timeout: Duration) -> Result<bool, ClientError> {
        let spec = Timespec {
            tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        loop {
            let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
            match poll(&mut fds, Some(&spec)) {
                Ok(0) => return Ok(false),
                Ok(_) => return Ok(true),
                Err(Errno::INTR) => {}
                Err(e) => return Err(ClientError::Io(e.into())),
            }
        }
    }

    fn read(&self) -> Result<Option<Received>, ClientError> {
        let mut buf = vec![0u8; MAX_DATAGRAM + 1];
        let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let got = match recvmsg(
            &self.fd,
            &mut [IoSliceMut::new(&mut buf)],
            &mut control,
            RecvFlags::DONTWAIT | RecvFlags::CMSG_CLOEXEC | RecvFlags::TRUNC,
        ) {
            Ok(got) => got,
            Err(Errno::AGAIN | Errno::INTR) => return Ok(None),
            Err(Errno::CONNRESET) => return Err(ClientError::Closed),
            Err(e) => return Err(ClientError::Io(e.into())),
        };
        if got.bytes == 0 {
            return Err(ClientError::Closed);
        }
        if got.bytes > MAX_DATAGRAM || got.flags.contains(ReturnFlags::TRUNC) {
            return Err(ClientError::Wire(WireError::Oversize(got.bytes)));
        }
        buf.truncate(got.bytes);
        let fd = control
            .drain()
            .filter_map(|message| match message {
                RecvAncillaryMessage::ScmRights(fds) => Some(fds),
                _ => None,
            })
            .flatten()
            .next();
        Ok(Some(Received { bytes: buf, fd }))
    }
}
