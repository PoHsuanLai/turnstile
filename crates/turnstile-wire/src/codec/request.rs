//! Client to service datagrams.

use super::reader::{Reader, check_len};
use super::{TAG_CREATE, TAG_DESTROY, TAG_EVENTS, WireError, create};
use crate::event::{EVENT_BYTES, Event};
use crate::limits::{MAX_DATAGRAM, MAX_EVENTS_PER_DATAGRAM, PROTOCOL_VERSION};
use crate::message::{DeviceId, Request};

/// Reads one datagram.
pub fn decode_request(bytes: &[u8]) -> Result<Request, WireError> {
    let mut r = Reader::new(bytes);
    let request = match r.header()? {
        TAG_CREATE => Request::Create(create::read(&mut r)?),
        TAG_EVENTS => {
            let device = DeviceId(r.u8()?);
            Request::Events {
                device,
                events: read_events(&mut r, MAX_EVENTS_PER_DATAGRAM)?,
            }
        }
        TAG_DESTROY => Request::Destroy {
            device: DeviceId(r.u8()?),
        },
        tag => return Err(WireError::Tag(tag)),
    };
    r.finish()?;
    Ok(request)
}

/// Writes one datagram; refuses what the other side would refuse.
pub fn encode_request(request: &Request) -> Result<Vec<u8>, WireError> {
    let mut out = vec![PROTOCOL_VERSION];
    match request {
        Request::Create(c) => {
            out.push(TAG_CREATE);
            create::write(c, &mut out)?;
        }
        Request::Events { device, events } => {
            check_len("the event list", events.len(), MAX_EVENTS_PER_DATAGRAM)?;
            if events.is_empty() {
                return Err(WireError::Truncated);
            }
            out.extend([TAG_EVENTS, device.0]);
            for e in events {
                out.extend_from_slice(&e.to_bytes());
            }
        }
        Request::Destroy { device } => out.extend([TAG_DESTROY, device.0]),
    }
    if out.len() > MAX_DATAGRAM {
        return Err(WireError::Oversize(out.len()));
    }
    Ok(out)
}

/// Events fill the rest of the datagram, [`EVENT_BYTES`] each; there is no count byte to disagree
/// with the length.
fn read_events(r: &mut Reader<'_>, max: usize) -> Result<Vec<Event>, WireError> {
    let rest = r.rest();
    if rest.is_empty() || !rest.len().is_multiple_of(EVENT_BYTES) {
        return Err(WireError::Truncated);
    }
    let got = rest.len() / EVENT_BYTES;
    if got > max {
        return Err(WireError::TooMany {
            what: "the event list",
            got,
            max,
        });
    }
    Ok(rest
        .as_chunks::<EVENT_BYTES>()
        .0
        .iter()
        .map(Event::from_bytes)
        .collect())
}
