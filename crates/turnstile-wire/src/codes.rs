//! The evdev numbers the ceilings are written in, each named once (`linux/input-event-codes.h`).

/// Event type: end of a frame.
pub const EV_SYN: u16 = 0x00;
/// Event type: key or button.
pub const EV_KEY: u16 = 0x01;
/// Event type: relative axis.
pub const EV_REL: u16 = 0x02;
/// Event type: absolute axis.
pub const EV_ABS: u16 = 0x03;
/// Event type: keyboard light.
pub const EV_LED: u16 = 0x11;

/// `SYN_REPORT`: the frame is complete.
pub const SYN_REPORT: u16 = 0;

/// Relative axis: pointer x.
pub const REL_X: u16 = 0x00;
/// Relative axis: pointer y.
pub const REL_Y: u16 = 0x01;
/// Relative axis: horizontal wheel steps.
pub const REL_HWHEEL: u16 = 0x06;
/// Relative axis: vertical wheel steps.
pub const REL_WHEEL: u16 = 0x08;
/// Relative axis: vertical wheel, high resolution.
pub const REL_WHEEL_HI_RES: u16 = 0x0b;
/// Relative axis: horizontal wheel, high resolution.
pub const REL_HWHEEL_HI_RES: u16 = 0x0c;
/// The highest relative axis code.
pub const REL_MAX: u16 = 0x0f;

/// Absolute axis: x.
pub const ABS_X: u16 = 0x00;
/// Absolute axis: y.
pub const ABS_Y: u16 = 0x01;
/// Multitouch: the slot the next events belong to.
pub const ABS_MT_SLOT: u16 = 0x2f;
/// Multitouch: x of the contact in the current slot.
pub const ABS_MT_POSITION_X: u16 = 0x35;
/// Multitouch: y of the contact in the current slot.
pub const ABS_MT_POSITION_Y: u16 = 0x36;
/// Multitouch: the contact's id, or -1 when it lifts.
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
/// The highest absolute axis code.
pub const ABS_MAX: u16 = 0x3f;

/// Button: the primary click.
pub const BTN_LEFT: u16 = 0x110;
/// Tool: one finger on the pad.
pub const BTN_TOOL_FINGER: u16 = 0x145;
/// Contact: something touches the pad.
pub const BTN_TOUCH: u16 = 0x14a;
/// Tool: two fingers.
pub const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
/// Tool: three fingers.
pub const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
/// Tool: four fingers.
pub const BTN_TOOL_QUADTAP: u16 = 0x14f;

/// The highest key or button code a keyboard device registers.
pub const KEY_MAX: u16 = 0x2ff;
/// Left control.
pub const KEY_LEFTCTRL: u16 = 29;
/// Left shift.
pub const KEY_LEFTSHIFT: u16 = 42;
/// Left alt.
pub const KEY_LEFTALT: u16 = 56;
/// Left super (`KEY_LEFTMETA`).
pub const KEY_LEFTMETA: u16 = 125;

/// Keyboard light: num lock.
pub const LED_NUML: u16 = 0;
/// Keyboard light: caps lock.
pub const LED_CAPSL: u16 = 1;
/// Keyboard light: scroll lock.
pub const LED_SCROLLL: u16 = 2;

/// Device property: a touchpad that moves a pointer, not a touchscreen.
pub const INPUT_PROP_POINTER: u16 = 0x00;
