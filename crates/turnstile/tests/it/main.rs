//! The service's tests: a fake kernel, a fake authority and sockets in scratch folders. No device,
//! no `/run`, no bus, no real `/proc`. One `#[ignore]` test makes a real device and is run by hand.

mod daemon;
mod device;
mod identity;
mod misc;
mod pending;
mod policy;
mod real_authority;
mod serve;
mod session;
mod slots;
mod support;
