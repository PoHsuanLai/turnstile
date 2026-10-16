//! The body of `Create`: the kind, then what the kind needs.

use super::WireError;
use super::reader::{Reader, check_len};
use crate::kind::Kind;
use crate::limits::{
    MAX_KEY_CODES, MAX_LABEL_BYTES, MAX_POINTER_ABS, MAX_POINTER_BUTTONS, MAX_POINTER_PROPS,
    MAX_POINTER_RELS,
};
use crate::message::{Create, Detail, PointerCaps};
use crate::spec::AbsAxis;

pub(super) fn read(r: &mut Reader<'_>) -> Result<Create, WireError> {
    let code = r.u8()?;
    let kind = Kind::from_code(code).ok_or(WireError::Kind(code))?;
    let detail = match kind {
        Kind::PalmrestKeys => {
            Detail::Keys(r.counted("the key list", MAX_KEY_CODES, Reader::u16)?)
        }
        Kind::KeycapPointer => Detail::Pointer(read_pointer(r)?),
        _ => Detail::None,
    };
    let create = Create { kind, detail };
    validate(&create)?;
    Ok(create)
}

fn read_pointer(r: &mut Reader<'_>) -> Result<PointerCaps, WireError> {
    let label_len = usize::from(r.u8()?);
    if label_len > MAX_LABEL_BYTES {
        return Err(WireError::Label);
    }
    let label = std::str::from_utf8(r.take(label_len)?)
        .map_err(|_| WireError::Label)?
        .to_owned();
    Ok(PointerCaps {
        label,
        vendor: r.u16()?,
        product: r.u16()?,
        props: r.counted("the property list", MAX_POINTER_PROPS, Reader::u16)?,
        buttons: r.counted("the button list", MAX_POINTER_BUTTONS, Reader::u16)?,
        rels: r.counted("the relative axis list", MAX_POINTER_RELS, Reader::u16)?,
        abs: r.counted("the absolute axis list", MAX_POINTER_ABS, read_axis)?,
    })
}

fn read_axis(r: &mut Reader<'_>) -> Result<AbsAxis, WireError> {
    Ok(AbsAxis {
        code: r.u16()?,
        minimum: r.i32()?,
        maximum: r.i32()?,
        fuzz: r.i32()?,
        flat: r.i32()?,
        resolution: r.i32()?,
    })
}

/// The detail must be the one the kind takes, within the caps and (for a label) plain text.
pub(super) fn validate(create: &Create) -> Result<(), WireError> {
    match (&create.kind, &create.detail) {
        (Kind::PalmrestKeys, Detail::Keys(codes)) => {
            check_len("the key list", codes.len(), MAX_KEY_CODES).map(drop)
        }
        (Kind::KeycapPointer, Detail::Pointer(caps)) => validate_pointer(caps),
        (Kind::PalmrestKeys | Kind::KeycapPointer, _) => Err(WireError::Detail),
        (_, Detail::None) => Ok(()),
        (_, _) => Err(WireError::Detail),
    }
}

fn validate_pointer(caps: &PointerCaps) -> Result<(), WireError> {
    if caps.label.len() > MAX_LABEL_BYTES || caps.label.chars().any(char::is_control) {
        return Err(WireError::Label);
    }
    check_len("the property list", caps.props.len(), MAX_POINTER_PROPS)?;
    check_len("the button list", caps.buttons.len(), MAX_POINTER_BUTTONS)?;
    check_len("the relative axis list", caps.rels.len(), MAX_POINTER_RELS)?;
    check_len("the absolute axis list", caps.abs.len(), MAX_POINTER_ABS).map(drop)
}

pub(super) fn write(create: &Create, out: &mut Vec<u8>) -> Result<(), WireError> {
    validate(create)?;
    out.push(create.kind.code());
    match &create.detail {
        Detail::None => {}
        Detail::Keys(codes) => {
            out.push(check_len("the key list", codes.len(), MAX_KEY_CODES)?);
            codes
                .iter()
                .for_each(|c| out.extend_from_slice(&c.to_le_bytes()));
        }
        Detail::Pointer(caps) => write_pointer(caps, out)?,
    }
    Ok(())
}

fn write_pointer(caps: &PointerCaps, out: &mut Vec<u8>) -> Result<(), WireError> {
    out.push(check_len("the label", caps.label.len(), MAX_LABEL_BYTES)?);
    out.extend_from_slice(caps.label.as_bytes());
    out.extend_from_slice(&caps.vendor.to_le_bytes());
    out.extend_from_slice(&caps.product.to_le_bytes());
    for list in [&caps.props, &caps.buttons, &caps.rels] {
        out.push(u8::try_from(list.len()).map_err(|_| WireError::Detail)?);
        list.iter()
            .for_each(|c| out.extend_from_slice(&c.to_le_bytes()));
    }
    out.push(u8::try_from(caps.abs.len()).map_err(|_| WireError::Detail)?);
    for axis in &caps.abs {
        out.extend_from_slice(&axis.code.to_le_bytes());
        for v in [
            axis.minimum,
            axis.maximum,
            axis.fuzz,
            axis.flat,
            axis.resolution,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(())
}
