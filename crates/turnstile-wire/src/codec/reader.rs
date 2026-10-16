//! A cursor over a datagram that never reads past its end.

use super::WireError;
use crate::limits::PROTOCOL_VERSION;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Reads the version and the tag.
    pub(crate) fn header(&mut self) -> Result<u8, WireError> {
        let version = match self.bytes.split_first() {
            None => return Err(WireError::Empty),
            Some((&first, rest)) => {
                self.bytes = rest;
                first
            }
        };
        if version != PROTOCOL_VERSION {
            return Err(WireError::Version(version));
        }
        self.u8()
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        if self.bytes.len() < n {
            return Err(WireError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, WireError> {
        self.take(1).map(|b| b[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, WireError> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, WireError> {
        self.take(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Everything not yet read.
    pub(crate) fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.bytes)
    }

    /// A count byte, checked against `max`, then that many items read by `item`.
    pub(crate) fn counted<T>(
        &mut self,
        what: &'static str,
        max: usize,
        mut item: impl FnMut(&mut Self) -> Result<T, WireError>,
    ) -> Result<Vec<T>, WireError> {
        let got = usize::from(self.u8()?);
        if got > max {
            return Err(WireError::TooMany { what, got, max });
        }
        (0..got).map(|_| item(self)).collect()
    }

    /// Fails if bytes are left.
    pub(crate) fn finish(&self) -> Result<(), WireError> {
        match self.bytes.len() {
            0 => Ok(()),
            n => Err(WireError::Trailing(n)),
        }
    }
}

/// Checks a list length against its cap on the way out.
pub(crate) fn check_len(what: &'static str, got: usize, max: usize) -> Result<u8, WireError> {
    if got > max {
        return Err(WireError::TooMany { what, got, max });
    }
    u8::try_from(got).map_err(|_| WireError::TooMany { what, got, max })
}
