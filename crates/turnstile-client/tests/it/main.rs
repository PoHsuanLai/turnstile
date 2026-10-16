//! The client's tests: a thread stands in for the service on a socket pair or a socket in a scratch
//! folder. No real service, no real device.

#[cfg(feature = "fake")]
mod fake;
mod real;
mod stand;
