//! Service to client datagrams.

use super::reader::{Reader, check_len};
use super::{TAG_CREATED, TAG_FEEDBACK, TAG_REFUSED, WireError};
use crate::event::Event;
use crate::kind::Delivery;
use crate::limits::{MAX_DATAGRAM, MAX_FEEDBACK_EVENTS, PROTOCOL_VERSION};
use crate::message::{DeviceId, Reason, Reply};

/// Reads one datagram.
pub fn decode_reply(bytes: &[u8]) -> Result<Reply, WireError> {
    let mut r = Reader::new(bytes);
    let reply = match r.header()? {
        TAG_CREATED => {
            let device = DeviceId(r.u8()?);
            let code = r.u8()?;
            let delivery = Delivery::from_code(code).ok_or(WireError::Delivery(code))?;
            Reply::Created { device, delivery }
        }
        TAG_FEEDBACK => {
            let device = DeviceId(r.u8()?);
            let events = r.counted("the feedback list", MAX_FEEDBACK_EVENTS, read_feedback)?;
            Reply::Feedback { device, events }
        }
        TAG_REFUSED => Reply::Refused(Reason::from_code(r.u8()?)),
        tag => return Err(WireError::Tag(tag)),
    };
    r.finish()?;
    Ok(reply)
}

fn read_feedback(r: &mut Reader<'_>) -> Result<Event, WireError> {
    Ok(Event::new(r.u16()?, r.u16()?, r.i32()?))
}

/// Writes one datagram.
pub fn encode_reply(reply: &Reply) -> Result<Vec<u8>, WireError> {
    let mut out = vec![PROTOCOL_VERSION];
    match reply {
        Reply::Created { device, delivery } => {
            out.extend([TAG_CREATED, device.0, delivery.code()]);
        }
        Reply::Feedback { device, events } => {
            out.extend([TAG_FEEDBACK, device.0]);
            out.push(check_len(
                "the feedback list",
                events.len(),
                MAX_FEEDBACK_EVENTS,
            )?);
            for e in events {
                out.extend_from_slice(&e.kind.to_le_bytes());
                out.extend_from_slice(&e.code.to_le_bytes());
                out.extend_from_slice(&e.value.to_le_bytes());
            }
        }
        Reply::Refused(reason) => out.extend([TAG_REFUSED, reason.code()]),
    }
    if out.len() > MAX_DATAGRAM {
        return Err(WireError::Oversize(out.len()));
    }
    Ok(out)
}
