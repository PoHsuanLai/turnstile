# Architecture

## What it is for

`/dev/uinput` lets a process create any input device. If the node is open to the logged-in user, any
program of that user can create a keyboard and type into the focused window, into a password prompt
or past the compositor's consent for virtual keyboards. turnstile is the only holder of the node
(`root:root 0600`, see `dist/72-turnstile.rules`). Two programs need virtual devices, so it makes
those and no others.

## Crates and seams

```
turnstile-wire    pure          kinds, ceilings, codec, rate limiter, key table
turnstile-client  rustix        Broker (the socket), FakeSource; palmrest and keycap link this
turnstile         rustix,       Daemon, serve, Session, policy; seams: Backend, Authority,
                  input-linux,  SeatSource, Clock
                  peephole
```

`scripts/check-boundary.sh` keeps the graph honest: the wire links no socket, device or identity
library; the client links no bus, async runtime or identity crate; the root service never links the
bus or an async runtime.

## One connection

```
socket unit (systemd) --accept--> Daemon --thread--> serve(conn)
                                                      |  Authority::identify   (pidfd, /proc, table)
                                                      |  loop: recv datagram -> decode_request
                                                      |        Session::handle -> Backend / ceiling
                                                      |        send replies (a handed fd rides with Created)
                                                      '- close: proxied devices die with the connection
```

- The service is socket-activated and exits after 60 s with no connection and nothing to serve
  (`Idle`). Devices it handed over belong to their holder and survive the exit. One thread per
  connection, at most 16, no async runtime.
- A new connection that makes no device within a second is closed (`HANDSHAKE_TIMEOUT_MS`); one
  that has a device may be silent for hours.
- The first datagram byte is the protocol version; every list in a datagram is counted and capped
  before it is allocated; an oversize datagram (read with `MSG_TRUNC`) ends the connection.

## The wire

`SOCK_SEQPACKET`, little endian, one message per datagram:

| Message | Direction | Body |
|---|---|---|
| `Create` | client to service | kind; for `PalmrestKeys` up to 64 key codes; for `KeycapPointer` a label, vendor, product, properties, buttons, relative axes, absolute axes |
| `Created` | service to client | device id, delivery; for a handed device the descriptor rides as `SCM_RIGHTS` |
| `Events` | client to service | device id, up to 64 `struct input_event` (24 bytes; the time is ignored) |
| `Feedback` | service to client | device id, up to 8 events; reserved, the service sends none today |
| `Destroy` | client to service | device id |
| `Refused` | service to client | reason (`Unknown`, `NotAllowed`, `TooMany`, `BadCaps`, `Unavailable`), then the service closes |

## Who may have what (`policy.rs`)

| Kind | Role in the table | Proof | User | Control group |
|---|---|---|---|---|
| palmrest touchpad, wheel, keys | `pad_daemon` | the image, a root-owned file in the table | the seat's active user | `.../user@<uid>.service/.../palmrest.service` |
| keycap keyboard, pointer | `keymap_daemon` | the image | not the seat's active user (a dynamic user); allowed before anyone is logged in | exactly `/system.slice/keycap.service` |

The cgroup of a system service cannot be forged by a user, so for keycap it is as strong as a root
daemon can get without asking a manager. For palmrest the residual gap is a process of the same user
that runs a copy of the genuine image with `LD_PRELOAD` from inside `palmrest.service`'s group; it gets
a touchpad, a wheel and keys that cannot type text, nothing else. The service does not ask the user's
manager: that would mean connecting to a socket the user controls from a root process.

## The ceilings (`turnstile-wire/src/ceiling/`)

| Kind | Delivery | Device and per-event rules |
|---|---|---|
| `PalmrestTouchpad` | proxied | exactly palmrest's capabilities: `BTN_LEFT`, `BTN_TOUCH`, `BTN_TOOL_FINGER..QUADTAP`, `ABS_X/Y`, `ABS_MT_SLOT` 0..4, `ABS_MT_TRACKING_ID` -1..0xffff, `ABS_MT_POSITION_X/Y`, `INPUT_PROP_POINTER`; ranges 0..4000 by 0..2800 (positions outside are clamped); a click only while a finger is down |
| `PalmrestWheel` | proxied | `REL_WHEEL`, `REL_HWHEEL` and the hi-res pair; magnitude at most 1200 |
| `PalmrestKeys` | proxied | only keys of palmrest's name table; no auto-repeat; at most 8 down; a letter, digit, punctuation, Enter, Space, Tab, Backspace, Delete or Escape only while control, alt or super is held |
| `KeycapKeyboard` | handed | codes 1 to 0x2ff, the six pointer axes of a combo device, the three lights; name `keycap keyboard`, vendor `0x4b43` |
| `KeycapPointer` | handed | the request cut to buttons, relative axes, absolute axes with sane ranges and pointer properties; never a key |

Rate limit per proxied device: 120 `Events` datagrams per second (burst 30). Above it datagrams are
dropped and counted; three seconds of sustained excess close the connection. Events refused by a
ceiling are counted; 256 over a connection's life close it.

Why palmrest's kinds are proxied and keycap's handed: a uinput descriptor cannot be made write-only.
The holder can `UI_DEV_DESTROY` it and set it up again with any capabilities. Handing palmrest the
descriptor would turn a bug in its input parsing into "types anything". keycap's keyboard is a full
keyboard by design, it polls the descriptor for lights, and proxying every keystroke would add a hop
for nothing, so its authority is the strong identity above.

Chord-only typing is a large step down from arbitrary typing, not a proof of safety (`Ctrl+M` is
Enter in a terminal). The gate for palmrest's kinds is identity; the ceiling keeps a compromised
palmrest from becoming a general typist.

## Tests

- `turnstile-wire`: tables for every ceiling row, the codec (round trips, every prefix, random bytes),
  the limiter, and a comparison of the key table with palmrest's source when it sits next to this one.
- `turnstile-client`: a thread stands in for the service on a socket pair or a socket in a scratch
  folder: create, split events, handed descriptors, refusals, timeouts, full queue, gone peer.
- `turnstile`: `Session` with a fake kernel (caps, flood, hostile events, failures); `serve` over a
  socket pair with the real client; the accept loop on a real socket (idle exit, the connection cap);
  the policy as a table; the real authority over a fixture tree standing for `/proc` and the table files; two `#[ignore]`d tests that make real devices.
