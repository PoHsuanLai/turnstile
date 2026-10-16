//! The service's side of a socket, by hand.

use rustix::net::{
    AddressFamily, RecvFlags, SendAncillaryBuffer, SendAncillaryMessage, SendFlags, SocketFlags,
    SocketType, recv, sendmsg, socketpair,
};
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, OwnedFd};
use turnstile_wire::{Reply, Request, decode_request, encode_reply};

/// A blocking socket pair: the client's end (it only ever sends with `DONTWAIT`) and ours.
pub(crate) fn pair() -> (OwnedFd, Stand) {
    let (client, service) = socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket pair");
    (client, Stand { sock: service })
}

#[derive(Debug)]
pub(crate) struct Stand {
    pub sock: OwnedFd,
}

impl Stand {
    /// The next datagram, decoded; `None` once the client has gone.
    pub(crate) fn next(&self) -> Option<Request> {
        let mut buf = [0u8; 4096];
        let (n, _) = recv(&self.sock, &mut buf[..], RecvFlags::empty()).expect("recv");
        (n > 0).then(|| decode_request(&buf[..n]).expect("a request that parses"))
    }

    pub(crate) fn reply(&self, reply: &Reply) {
        let bytes = encode_reply(reply).expect("encodes");
        rustix::net::send(&self.sock, &bytes, SendFlags::NOSIGNAL).expect("send");
    }

    pub(crate) fn reply_with_fd(&self, reply: &Reply, fd: &OwnedFd) {
        let bytes = encode_reply(reply).expect("encodes");
        let fds = [fd.as_fd()];
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        assert!(control.push(SendAncillaryMessage::ScmRights(&fds)));
        sendmsg(
            &self.sock,
            &[IoSlice::new(&bytes)],
            &mut control,
            SendFlags::NOSIGNAL,
        )
        .expect("sendmsg");
    }
}
