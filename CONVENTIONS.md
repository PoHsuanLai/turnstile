# Conventions

turnstile follows the program's shared conventions: small typed structs, enums instead of `bool`
fields and parameters, typed errors (`thiserror`), no `unsafe`, short files, doc comments on every
public item, tests that never touch the real system. `ARCHITECTURE.md` here has the crate map and
the seams.

1. **Library first.** `turnstile-wire` is pure and usable on its own; `turnstile-client` is a small
   library that palmrest and keycap link; `turnstile` is a library with a thin `main.rs`. Every real
   effect (the kernel, the caller's identity, the seat, the clock) sits behind a trait with a fake in
   the tests: `Backend`, `Authority`, `SeatSource`, `Clock`, `DeviceSource`.
2. **Wire values stay at the edge.** Datagram bytes become enums and structs where they are read
   (`turnstile-wire/src/codec/`); nothing past the codec sees a `u8` that means something except
   through a named type (`Kind`, `Reason`, `DeviceId`, `Delivery`).
3. **No panics on a peer.** A malformed, oversize, early or hostile datagram is a refusal with a
   named reason and the connection closed, never a panic. `unwrap` and `expect` only in tests and
   in the fakes. In release a panic aborts the process (`panic = "abort"`); socket activation brings
   the service back, and a half-working service is the worse failure.
4. **Everything a peer can make us hold is capped,** by a named constant in `turnstile-wire/src/limits.rs`
   with the reason beside it, and a peer that exceeds one is refused or cut, never allowed to wedge
   the service. A new list, queue or counter that a peer can grow gets its constant in the same
   change.
5. **Time, files and the environment are arguments.** `turnstile-wire` reads no clock, file or
   socket. In `turnstile`, only `main.rs`, `clock.rs`, `activation.rs`, `identity/seat.rs` and
   `identity/peephole_authority.rs` read the real clock, environment or
   files; the rest take them as trait objects or arguments.
6. **A ceiling is data, not code at the call site.** What a device kind may have and what its events
   may be is written once, in `turnstile-wire/src/ceiling/`, with a table test per row. The daemon
   never takes a name, id, axis range or property from a client except through that module.
7. **Trust.** A caller is whoever the kernel says is behind the socket (a pidfd), judged against a
   root-owned table (`peephole` refuses a table file, or a folder above it, that anyone but root can
   write), never what it claims. Nothing in this repository adds a user-writable input to that
   decision.
8. **Plain language.** Nothing a person sees names a protocol, a bus or a developer tool. This
   service shows people nothing; its log lines are for the journal, and the clients that surface a
   failure use their own words.
9. **Licence hygiene.** turnstile is MIT OR Apache-2.0. Dependencies must pass `deny.toml`; no
   copyleft code is linked or copied. Other uinput tools (ydotool, evsieve, the kernel samples) are
   read for facts only, never for code.
10. **`#[non_exhaustive]`** on public enums that will grow; `Copy` only for heap-free values of at
    most two words. A `match` on one from another crate ends in an arm that refuses.
11. **`cargo fmt -p <crate>`** to write; the gate runs `cargo fmt --all --check`.
12. **One test binary per crate** (`tests/it/main.rs`), tables over one-off cases. A test that
    makes a real device is `#[ignore]`d with the reason, and lives on the owner's manual queue.
