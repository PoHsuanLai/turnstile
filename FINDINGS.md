# Findings

Departures from the plan, standing facts and open items. An entry names the condition that closes it.

## Decisions and departures

1. **The service keeps `CAP_SYS_PTRACE` (owner, 2026-10-16).** Reading `/proc/<pid>/exe` of palmrest,
   which runs as another user, needs it. Without it only the control group is available and any
   process of the user that moves itself into `palmrest.service`'s group can make the pad devices.
   Mitigations are in the unit: no `ptrace`, `process_vm_*` or `pidfd_getfd` system call, a
   read-only `/proc`, no network, and a parser that takes only fixed-size fields. Closes if the owner
   decides the image check is not worth the capability.
2. **keycap's identity is the strong one, the keyboard is handed over, the pad is proxied (owner,
   2026-10-16).** A uinput descriptor cannot be made write-only (the holder can destroy the device and
   set it up again with any capabilities), so a handed descriptor is only as safe as its holder.
   keycap is a system service with a control group a user cannot enter, so it gets the fd. palmrest
   runs as the user, so its events pass the ceiling one by one.
3. **No `RuntimeDirectory=` in the service unit.** The design listed it; stopping the service would
   delete `/run/turnstile`, and with it the socket systemd keeps for activation. The socket unit makes
   the folder (`DirectoryMode=0755`) and nothing else needs one.
4. **`MaxConnections=` is not in the socket unit.** It applies to `Accept=yes` sockets only. The
   service caps its own live connections at 16 (`limits::MAX_CONNECTIONS`) and answers the rest with
   `Refused(TooMany)`.
5. **`LimitNOFILE=128`, not 64.** Sixteen connections plus up to four proxied devices on each is 80
   descriptors in the worst case; 64 would make the cap unreachable.
6. **`Feedback` is on the wire but the service sends none.** No proxied kind has an output
   capability (no lights, no repeat, no sound), so there is nothing to forward. keycap's lights
   travel on the descriptor it is handed. The message exists so a future kind with feedback needs no
   protocol change.
7. **A keycap pointer carries a label, vendor and product and the original's axis ranges.** The design
   said the service takes no name, ids or absinfo from a client. keycap's twin of a combo device must
   keep the original's vendor and product so the desktop's per-device quirks still match, and a
   touchpad's absolute axes with their ranges. So the pointer request carries them, cut to the
   ceiling: only buttons (`BTN_MISC` to `BTN_GEAR_UP` and the `0x2c0` block, never a key), relative
   axes up to `REL_MAX`, absolute axes up to `ABS_MAX` with ranges within 2^24, properties up to
   `INPUT_PROP_POINTING_STICK`; the name is the label (at most 62 bytes) plus the fixed suffix
   `(keycap pointer)`, which keycap never grabs. Because the descriptor is handed over, the ceiling
   shapes the device the service makes and is not a boundary on what the holder can then do
   (decision 2). Closes if keycap stops cloning axis ranges.
8. **Palmrest's keyboard classes are stricter than the design's list.** Besides letters, digits, Enter,
   Space, Backspace, Delete and Escape, Tab and the punctuation keys also need a control, alt or super
   held. Navigation, function keys, Insert, Back and Forward are free. palmrest's real combinations
   (alt+left, alt+right, super+pageup, ctrl+shift+t, f5) all pass.
9. **A position outside the touchpad's range is clamped, not refused.** The kernel does not clamp
   uinput positions; palmrest's spread of up to five contacts could exceed the range by a few units at
   an edge, and dropping half a frame is worse than moving a coordinate to the edge. A slot or a
   tracking id outside its range is refused.
10. **`fake` code is compiled always.** `backend::fake`, `identity::fake`, `clock::ManualClock` and the
    client's `fake` are small test doubles that the integration tests (a separate crate) and the
    tests of palmrest and keycap need. A cargo feature would make `cargo test` skip the tests
    silently when the feature is off. Nothing shipped calls them.

## Unverified (nothing here was compiled or run; the gate checks each)

1. peephole's API was read from the sibling lane's tree at its commit `e3f8ed3`; the dependency is
   the git tag `v0.1.0` (`peephole` and `peephole-core` with feature `desktop`), commit `9cef560`,
   which was not re-read after tagging. The calls are
   confined to `crates/turnstile/src/identity/peephole_authority.rs` and `policy.rs`:
   `Pinned::of_stream`, `Prober::system`, `Prober::probe`, `Prober::fs`, `TableFiles::desktop` and
   `TableFiles::load`, `judge`, `Standing::Service`, `roles::{PAD_DAEMON, KEYMAP_DAEMON}`. The lane
   was still moving; if it changes a signature the change is in those two files. `Pinned::of_stream`
   gives `Pin::Reopened` (rustix 1.1 has no safe `SO_PEERPIDFD`), which this service accepts: it
   asks for `Proof::Exe`, not for a firm pin.
2. `input-linux` 0.7.1's `UInputHandle::create` calls `UI_DEV_SETUP`, then `UI_ABS_SETUP` for each axis,
   then `UI_DEV_CREATE`; that the kernel accepts the axes after `UI_DEV_SETUP` (it does in
   `drivers/input/misc/uinput.c`, `uinput_abs_setup` only needs the absbit and a device not yet created).
3. The unit's `SystemCallFilter` with the minus-list, `ReadOnlyPaths=/proc` and `CAP_SYS_PTRACE` alone
   is enough to read another user's `/proc/<pid>/exe`: start it in a Fedora, Ubuntu and Arch VM and run
   `systemd-analyze security turnstile.service`.
4. `rustix` 1.1: `recv` returns `(output, real_len)`; `RecvFlags::TRUNC` on `SOCK_SEQPACKET`
   reports the real length; `SO_RCVTIMEO` on an accepted socket ends `recv` with `EAGAIN`.
5. `SO_PEERPIDFD` needs kernel 6.5 (peephole's `Pinned::of_stream`); an older kernel refuses every
   caller, which is the intended fail-closed behaviour.
6. SELinux on Fedora lets an unconfined service domain open `/dev/uinput` and `/proc/<pid>/exe` of a
   user process.

## Deferred

- **Socket peers are pinned with `Pin::Reopened`, not `Pin::AtConnect`.** `Pinned::of_stream` reads
  `SO_PEERCRED`, opens a pidfd and re-reads to compare, because rustix 1.1.5 has no safe
  `SO_PEERPIDFD`. The service therefore never asks for `PinNeed::AtConnect` (it judges a `Standing` and
  needs `Proof::Exe`, not a pin level). The remaining gap is a connecting process that dies and has its
  pid reused within the milliseconds before the pidfd opens. Closes when rustix gains the wrapper and
  peephole returns `AtConnect`: then require it in `policy.rs`.

## Open items

- The units, rule and modules file are placed by the desktop installer (`pa2-desk`); this repository
  only ships them in `dist/`.
- palmrest and keycap switch over (`pa2-palm`, `pa2-key`); their briefs are in the lane report.
