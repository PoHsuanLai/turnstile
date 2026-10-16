//! Every cap, each with the reason it has this number. A peer that exceeds one is refused or cut,
//! never allowed to make the service store or wait for more.

/// The first byte of every datagram. A change of meaning bumps it; the service refuses others.
pub const PROTOCOL_VERSION: u8 = 1;

/// The largest datagram either side sends or accepts: an `Events` of 64 events is 1539 bytes.
pub const MAX_DATAGRAM: usize = 2048;
/// Events in one datagram: a touchpad frame is 8 to 12 events, so 64 holds several frames.
pub const MAX_EVENTS_PER_DATAGRAM: usize = 64;
/// Feedback events in one datagram: a keyboard has three lights.
pub const MAX_FEEDBACK_EVENTS: usize = 8;

/// Key codes palmrest's keyboard may ask for: its gesture table has a few dozen at most.
pub const MAX_KEY_CODES: usize = 64;
/// Keys held at once on palmrest's keyboard: a combination is three or four.
pub const MAX_KEYS_DOWN: usize = 8;
/// Buttons on a pointer: the mouse block of `linux/input-event-codes.h` is 16 wide.
pub const MAX_POINTER_BUTTONS: usize = 16;
/// Relative axes on a pointer: the kernel defines 16 and a mouse uses a handful.
pub const MAX_POINTER_RELS: usize = 8;
/// Absolute axes on a pointer: a multitouch pad has about a dozen.
pub const MAX_POINTER_ABS: usize = 16;
/// Properties on a pointer: a pad is a pointer and perhaps a button pad.
pub const MAX_POINTER_PROPS: usize = 8;
/// Bytes of a pointer's label: a uinput name is 80 bytes, and the suffix and a space take 17 and the
/// terminator one.
pub const MAX_LABEL_BYTES: usize = 62;

/// Devices one connection may have at once.
pub const MAX_DEVICES_PER_CONNECTION: usize = 4;
/// Devices one connection may ask for in its life: ids are one byte and are never reused.
pub const MAX_CREATES_PER_CONNECTION: u8 = 64;
/// Handed devices one connection may ask for: the service cannot see them close, so it counts them.
pub const MAX_HANDED_PER_CONNECTION: u8 = 16;
/// Connections at once: the units run one palmrest and one keycap, so 16 is generous.
pub const MAX_CONNECTIONS: usize = 16;

/// `Events` datagrams per second per device: a touchpad frame at 120 Hz is one datagram.
pub const EVENT_DATAGRAMS_PER_SECOND: u64 = 120;
/// Datagrams that may arrive in a burst before the rate applies.
pub const EVENT_BURST: u64 = 30;
/// A connection that keeps exceeding the rate for this long is cut.
pub const FLOOD_CUT_MS: u64 = 3_000;
/// A pause in the excess this long ends one flood and starts another count.
pub const FLOOD_GAP_MS: u64 = 1_000;
/// Events refused by a ceiling over a connection's life before it is cut: a bug sends a few, a
/// hostile peer sends them all.
pub const MAX_DENIED_EVENTS: u32 = 256;

/// A new connection that has made no device in this long is closed.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 1_000;
/// The largest wheel step in one event: a fast flick is a few hundred hi-res units.
pub const MAX_WHEEL_MAGNITUDE: i32 = 1_200;
