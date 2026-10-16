//! The real kernel side: `/dev/uinput` through `input-linux`'s safe wrappers of the ioctls.

use super::{Backend, BackendError, Device};
use input_linux::{
    AbsoluteAxis, AbsoluteInfo, AbsoluteInfoSetup, EventKind, EventTime, InputId, InputProperty,
    Key, LedKind, RangeError, RelativeAxis, UInputHandle, sys,
};
use rustix::fs::{Mode, OFlags};
use std::io;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use turnstile_wire::{AbsAxis, DeviceSpec, Event};

/// The uinput control node.
pub const UINPUT: &str = "/dev/uinput";
/// `BUS_VIRTUAL`.
const BUS_VIRTUAL: u16 = 0x06;

/// Makes devices through the uinput node.
#[derive(Debug, Clone)]
pub struct UinputBackend {
    node: PathBuf,
}

impl Default for UinputBackend {
    fn default() -> Self {
        Self {
            node: PathBuf::from(UINPUT),
        }
    }
}

impl Backend for UinputBackend {
    type Device = UinputDevice;

    fn create(&self, spec: &DeviceSpec) -> Result<UinputDevice, BackendError> {
        let fd = rustix::fs::open(
            self.node.as_path(),
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| BackendError::Create(e.into()))?;
        let handle = UInputHandle::new(fd);
        // Should anything fail from here the handle drops, the descriptor closes and the kernel
        // removes whatever was half made.
        register(&handle, spec).map_err(BackendError::Create)?;
        Ok(UinputDevice { handle })
    }
}

/// A device made through the uinput node.
#[derive(Debug)]
pub struct UinputDevice {
    handle: UInputHandle<OwnedFd>,
}

impl Device for UinputDevice {
    fn write(&mut self, events: &[Event]) -> Result<(), BackendError> {
        let raw = events
            .iter()
            .map(raw_event)
            .collect::<io::Result<Vec<sys::input_event>>>()
            .map_err(BackendError::Write)?;
        let written = self.handle.write(&raw).map_err(BackendError::Write)?;
        if written == raw.len() {
            Ok(())
        } else {
            Err(BackendError::Write(io::Error::new(
                io::ErrorKind::WriteZero,
                "short write to the device",
            )))
        }
    }

    fn into_fd(self) -> OwnedFd {
        self.handle.into_inner()
    }
}

fn range(_: RangeError) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "a code outside the kernel's range",
    )
}

fn raw_event(event: &Event) -> io::Result<sys::input_event> {
    let kind = EventKind::from_type(event.kind).map_err(range)?;
    Ok(input_linux::InputEvent {
        time: EventTime::new(0, 0),
        kind,
        code: event.code,
        value: event.value,
    }
    .into_raw())
}

fn axis(axis: &AbsAxis) -> io::Result<AbsoluteInfoSetup> {
    Ok(AbsoluteInfoSetup {
        axis: AbsoluteAxis::from_code(axis.code).map_err(range)?,
        info: AbsoluteInfo {
            value: 0,
            minimum: axis.minimum,
            maximum: axis.maximum,
            fuzz: axis.fuzz,
            flat: axis.flat,
            resolution: axis.resolution,
        },
    })
}

/// Declares every capability of `spec`, then creates the device.
fn register(handle: &UInputHandle<OwnedFd>, spec: &DeviceSpec) -> io::Result<()> {
    handle.set_evbit(EventKind::Synchronize)?;
    for kind in spec.event_types() {
        handle.set_evbit(EventKind::from_type(kind).map_err(range)?)?;
    }
    for &code in &spec.keys {
        handle.set_keybit(Key::from_code(code).map_err(range)?)?;
    }
    for &code in &spec.rels {
        handle.set_relbit(RelativeAxis::from_code(code).map_err(range)?)?;
    }
    let axes = spec.abs.iter().map(axis).collect::<io::Result<Vec<_>>>()?;
    for setup in &axes {
        handle.set_absbit(setup.axis)?;
    }
    for &code in &spec.props {
        handle.set_propbit(InputProperty::from_code(code).map_err(range)?)?;
    }
    for &code in &spec.leds {
        handle.set_ledbit(LedKind::from_code(code).map_err(range)?)?;
    }
    let id = InputId {
        bustype: BUS_VIRTUAL,
        vendor: spec.vendor,
        product: spec.product,
        version: 1,
    };
    handle.create(&id, spec.name.as_bytes(), 0, &axes)
}
