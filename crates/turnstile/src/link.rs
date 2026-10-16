//! The service's end of one `SOCK_SEQPACKET` connection: whole datagrams in, whole datagrams out,
//! and the descriptor that rides with a `Created` of a handed device.

use rustix::io::Errno;
use rustix::net::sockopt::{Timeout, set_socket_timeout};
use rustix::net::{
    RecvFlags, SendAncillaryBuffer, SendAncillaryMessage, SendFlags, recv, send, sendmsg,
};
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, OwnedFd};
use std::time::Duration;
use turnstile_wire::limits::MAX_DATAGRAM;
use turnstile_wire::{Reply, WireError, encode_reply};

/// What the next read of the connection gave.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// One datagram of this many bytes is in the buffer.
    Datagram(usize),
    /// The peer closed the connection, or it broke.
    Closed,
    /// The datagram was longer than any message.
    Oversize,
    /// The receive timeout passed with nothing to read.
    TimedOut,
}

/// A buffer for one datagram, one byte longer than any message so that overlong ones are seen.
#[derive(Debug)]
pub struct Inbox {
    buf: Vec<u8>,
}

impl Inbox {
    /// An empty buffer.
    pub fn new() -> Self {
        Self {
            buf: vec![0; MAX_DATAGRAM + 1],
        }
    }

    /// The first `len` bytes of the last datagram.
    pub fn datagram(&self, len: usize) -> &[u8] {
        self.buf.get(..len).unwrap_or_default()
    }

    /// Waits for the next datagram (up to the connection's receive timeout).
    pub fn receive(&mut self, conn: &OwnedFd) -> Incoming {
        loop {
            return match recv(conn, &mut self.buf[..], RecvFlags::TRUNC) {
                Ok((0, 0)) => Incoming::Closed,
                Ok((_, real)) if real > MAX_DATAGRAM => Incoming::Oversize,
                Ok((len, _)) => Incoming::Datagram(len),
                Err(Errno::INTR) => continue,
                Err(Errno::AGAIN) => Incoming::TimedOut,
                Err(_) => Incoming::Closed,
            };
        }
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a reply was not delivered.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SendError {
    /// The peer's queue is full: it is not reading.
    #[error("the peer is not reading")]
    Full,
    /// The peer is gone.
    #[error("the peer is gone")]
    Gone,
    /// The reply does not encode.
    #[error("the reply does not encode: {0}")]
    Wire(#[from] WireError),
}

fn map_errno(e: Errno) -> SendError {
    match e {
        Errno::AGAIN => SendError::Full,
        _ => SendError::Gone,
    }
}

/// Sends `reply`; never blocks.
pub fn send_reply(conn: &OwnedFd, reply: &Reply) -> Result<(), SendError> {
    let bytes = encode_reply(reply)?;
    send(conn, &bytes, SendFlags::DONTWAIT | SendFlags::NOSIGNAL)
        .map(drop)
        .map_err(map_errno)
}

/// Sends `reply` with `device` riding along as a descriptor; never blocks.
pub fn send_reply_with_fd(
    conn: &OwnedFd,
    reply: &Reply,
    device: &OwnedFd,
) -> Result<(), SendError> {
    let bytes = encode_reply(reply)?;
    let fds = [device.as_fd()];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err(SendError::Gone);
    }
    sendmsg(
        conn,
        &[IoSlice::new(&bytes)],
        &mut control,
        SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
    )
    .map(drop)
    .map_err(map_errno)
}

/// Sets how long `Inbox::next` may wait; `None` waits for ever.
pub fn set_receive_timeout(conn: &OwnedFd, timeout: Option<Duration>) -> Result<(), Errno> {
    set_socket_timeout(conn, Timeout::Recv, timeout)
}
