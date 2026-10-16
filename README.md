# turnstile

A small root service that creates only allowed virtual input devices for named desktop programs.

On Linux, a program that wants to move the pointer, scroll or press a shortcut by itself needs
`/dev/uinput`. Giving that node to the logged-in user (the usual `uaccess` rule) lets every program of
that user create a keyboard and type into whatever has focus, past any consent the desktop asks for.
turnstile closes the node to everyone but itself and makes devices for two known programs:

| Program | Devices | How |
|---|---|---|
| palmrest (Magic Mouse gestures) | a touchpad, a scroll wheel, a keyboard for shortcut combinations | proxied: palmrest sends events, turnstile checks each one against a ceiling and writes it |
| keycap (Mac keyboard mapping) | a full keyboard with its lights, the pointer half of a combo device | handed: turnstile makes the device and passes keycap its file descriptor |

Who is calling is decided by the kernel (the process behind the socket, held by a pidfd) and a
root-owned table of desktop programs read through [peephole](https://github.com/PoHsuanLai/peephole),
not by anything the caller says. A kind of device is only made for the program it belongs to.

## Crates

| Crate | What |
|---|---|
| `turnstile-wire` | Pure. The datagrams, their codec, the fixed list of device kinds and the per-event ceilings. |
| `turnstile-client` | What palmrest and keycap link. Sync and small: connect, ask for a device, send events, get a descriptor. A `DeviceSource` trait with the real service and a fake. |
| `turnstile` | The root service: socket activation, caller identity, the device life cycle, exit after 60 s idle. |

```rust
use turnstile_client::{Broker, Create, DeviceSource, Made, Session};

let mut session = Broker::default().open()?;       // /run/turnstile/input.sock
let Made::Proxied(pad) = session.create(&Create::touchpad())? else { unreachable!() };
session.send(pad, &frame)?;                        // Sent::Queued, or Sent::Dropped when the service is behind
let Made::Handed { fd, .. } = session.create(&Create::keyboard())? else { unreachable!() };
```

## Files it ships (`dist/`)

| File | Placed at |
|---|---|
| `turnstile.socket`, `turnstile.service` | `/usr/lib/systemd/system/` |
| `72-turnstile.rules` | `/etc/udev/rules.d/`: `/dev/uinput` becomes `root:root 0600` and loses the `uaccess` tag |
| `turnstile-modules.conf` | `/etc/modules-load.d/turnstile.conf` |
| the table of desktop programs | `/etc/quire/desktop-callers.toml`, from the desktop release |

The desktop installer places them; to try it by hand, copy them, then
`systemctl daemon-reload`, `udevadm control --reload-rules && udevadm trigger --subsystem-match=misc`,
`systemctl enable --now turnstile.socket`.

## Tests

Nothing touches the real system: a fake kernel behind a trait, sockets in scratch folders, tables for
every ceiling. The two tests that make a real device (`crates/turnstile/tests/it/device.rs`) are
`#[ignore]`d and run by hand on a machine with `/dev/uinput`.

See `ARCHITECTURE.md` for the design and `FINDINGS.md` for the decisions and what is unverified.

Licensed under either of MIT or Apache-2.0, at your option.
