//! Datagrams to messages and back. The first byte is [`PROTOCOL_VERSION`], the second the message
//! tag; all integers are little endian. Every decoder is total: bad input is a [`WireError`],
//! never a panic, and every count is checked against its cap before anything is allocated.

mod create;
mod reader;
mod reply;
mod request;

pub use reply::{decode_reply, encode_reply};
pub use request::{decode_request, encode_request};

/// Why a datagram could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WireError {
    /// Nothing was sent.
    #[error("the message is empty")]
    Empty,
    /// The sender speaks another version.
    #[error("protocol version {0} is not supported")]
    Version(u8),
    /// The message type is not known.
    #[error("message type {0} is not known")]
    Tag(u8),
    /// The datagram ends before the message does.
    #[error("the message ends early")]
    Truncated,
    /// Bytes follow the end of the message.
    #[error("{0} bytes follow the end of the message")]
    Trailing(usize),
    /// A list is longer than its cap.
    #[error("{what} has {got} entries and at most {max} are allowed")]
    TooMany {
        /// Which list.
        what: &'static str,
        /// How many it has.
        got: usize,
        /// The cap.
        max: usize,
    },
    /// The device kind is not known.
    #[error("device kind {0} is not known")]
    Kind(u8),
    /// The delivery byte is not known.
    #[error("delivery {0} is not known")]
    Delivery(u8),
    /// The label is not plain text of the allowed length.
    #[error("the label is not plain text of at most 62 bytes")]
    Label,
    /// The encoded message would not fit one datagram.
    #[error("a message of {0} bytes does not fit one datagram")]
    Oversize(usize),
}

pub(crate) const TAG_CREATE: u8 = 1;
pub(crate) const TAG_EVENTS: u8 = 2;
pub(crate) const TAG_DESTROY: u8 = 3;
pub(crate) const TAG_CREATED: u8 = 1;
pub(crate) const TAG_FEEDBACK: u8 = 2;
pub(crate) const TAG_REFUSED: u8 = 3;
