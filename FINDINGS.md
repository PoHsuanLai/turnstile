# Findings

Departures from the plan, standing facts and open items. An entry names the condition that closes it.

## Decisions and departures

1. **The service keeps `CAP_SYS_PTRACE` (owner, 2026-10-16).** Reading `/proc/<pid>/exe` of palmrest,
   which runs as another user (a dynamic user, since palmrest became a system service), needs it. Without
   it only the control group is available; both groups are system ones now, so the image check is
   defence in depth.
   Mitigations are in the unit: no `ptrace`, `process_vm_*` or `pidfd_getfd` system call, a
   read-only `/proc`, no network, and a parser that takes only fixed-size fields. Closes if the owner
   decides the image check is not worth the capability.
   The process's own `environ` is not read (NEW-FA-1, owner 2026-10-17): it is mode 0400 and owned
   by the dynamic user, and the kernel checks that mode before ptrace access, so this capability
   alone cannot open it. Opening it would take `CAP_DAC_READ_SEARCH` (every file) or reading the
   process's memory (every process's); only root can set a system unit's environment, and the unit's
   root-owned files must unset every loader variable, so the check is dropped instead.
2. **keycap's identity is the strong one, the keyboard is handed over, the pad is proxied (owner,
   2026-10-16).** A uinput descriptor cannot be made write-only (the holder can destroy the device and
   set it up again with any capabilities), so a handed descriptor is only as safe as its holder.
   keycap is a system service with a control group a user cannot enter, so it gets the fd. palmrest
   is a system service too since 2026-10-17 (owner decision) and its identity is as strong, but its
   events still pass the ceiling one by one: the ceiling bounds what a bug in its input parsing can type.
3. **No `RuntimeDirectory=` in the service unit.** The design listed it; stopping the service would
   delete `/run/turnstile`, and with it the socket systemd keeps for activation. The socket unit makes
   the folder (`DirectoryMode=0755`) and nothing else needs one.
4. **`MaxConnections=` is not in the socket unit.** It applies to `Accept=yes` sockets only. The
   service caps its own live connections and answers the rest with `Refused(TooMany)`: 16 for strangers
   in all and 4 per user, plus a reserve of 4 for each of palmrest and keycap (NEW-PI-6, `slots.rs`).
5. **`LimitNOFILE=128`, not 64.** Twenty-four connections (16 strangers, 8 reserved) plus up to four proxied devices on each is 120
   descriptors in the worst case (strangers make none, so the real bound is far lower); 64 would make the cap unreachable.
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
10. **`fake` code is behind the `fake` feature.** The daemon's fake kernel and identity (since
    v0.1.0) and the client's `fake` (since v0.1.1) are compiled only with `fake`, so a shipped build
    cannot hold a stand-in for the service. palmrest and keycap name the feature on their
    dev-dependency, so their tests always have it; this repo's gate runs `--all-features`, and a plain
    `cargo test` skips the client's `fake` tests.

11. **Full proof for both clients (NEW-PI-4, 2026-10-17).** palmrest and keycap are admitted at peephole's
    `Full` only. With no manager to ask, `identity/unit_files` supplies the unit's facts from the unit
    files and the process (ARCHITECTURE "Who may have what"). `MainPID` is taken as the caller's own pid.
    That was a residual for palmrest while it was a user unit (a same-user process could fill the
    group); palmrest is a system unit since 2026-10-17 (`/system.slice/palmrest.service`, as keycap), so
    it is closed: the group is the system manager's alone.
    A symlink as the unit's fragment reads as writable (a link's mode is 0777), so it is refused; our
    units are plain files.

12. **Identification runs on the connection's thread, with a pending cap (NEW-FA-3, 2026-10-17).**
    Naming a peer reads its `/proc` entries and stats its image, and a program running from a file
    system it controls (FUSE) can make that wait for ever; on the single accept thread that stalled
    every later accept, palmrest's and keycap's included. Now the accept loop reads only the peer's
    user from `SO_PEERCRED` (no `/proc`), takes a ticket from `pending.rs` (2 per user, 6 in all,
    `MAX_PENDING_PER_USER`, `MAX_PENDING_IDENTIFICATIONS`) and spawns the connection thread, which
    identifies and then takes its slot (`admit.rs`), so NEW-PI-6 holds as before: strangers are
    counted into their own pool and never into the reserve. A user over its share is closed at once.
    Choice: a per-connection thread rather than a worker pool or a timeout, because a blocked `stat`
    cannot be cancelled, so only a thread of its own keeps the stall off everyone else; the cap
    bounds what stalled threads can cost. Budget: 1 loop + 16 strangers + 8 reserved + 6 pending = 31
    threads under `TasksMax=32` (a compile-time check in the limits). Descriptors: a pending
    connection holds its socket, a pidfd and a file or two at a time, about 24 more at the cap; the
    128 still holds, since strangers make no devices (16 + 8 x 5 + 24 + a few = about 90; the
    figure of 120 in decision 5 counted devices for strangers too).
    Residual: stuck identifications are never reclaimed, so accounts that each run a hanging program
    can use up their 2 each; with 3 such accounts the 6 are gone and no new connection (palmrest's
    included) is served until those peers exit. A hung peer also keeps the service from exiting idle.
    That takes several local accounts, not one. Closes if the pending room is split so that a user
    already proven as palmrest or keycap has its own (needs the policy's uid, a config the service
    does not have).

13. **Every device carries the label of the service it was made for (owner, 2026-10-17, NEW-FA-4).**
    casement used to guess whether input is the person's own hand from a device's bus type and
    vendor id, which wrongly caught a laptop's keyboard and kept palmrest's Magic Mouse clicks off
    the lock screen and consent sheets. turnstile is the only process that can create a uinput
    device (`dist/72-turnstile.rules`) and it already proves which service each device is for
    (`policy.rs`), so it writes that into the device with `UI_SET_PHYS` (`input-linux`'s
    `UInputHandle::set_phys`, before `UI_DEV_SETUP`): `turnstile/pad_daemon` for palmrest's three kinds,
    `turnstile/keymap_daemon` for keycap's two. The kernel shows it in
    `/sys/class/input/<event>/device/phys`. The format is `turnstile_wire::Maker` (format and
    `parse`); every plan sets `DeviceSpec::maker` and a test pins the exact strings and the label of
    each kind. casement links nothing of turnstile, so it keeps its own copy of the parser and pins the
    same strings in its `origin.rs` test: change a string here and change it there. The label is
    not a secret and not signed: it is trusted because only root can create the device. A device made
    some other way (a `uinput` group member on a standalone install, ydotool) has none.

14. **Plain types (owner, 2026-10-28).** The type rule changed (CONVENTIONS 13). `Verdict` became
    `Result<Event, Denial>`, `Handled` a `Result` over `Outgoing`/`Refused`, `Idling` a `bool`,
    `Phase` a `bool` method, `Flood` an `Option`, and `Credential` is a struct held as an `Option`.
    No behaviour, wire or table change. The Proxied-or-Handed mirrors (`Route`, `Made`) wait for
    palmrest's wave.

## Unverified (nothing here was compiled or run; the gate checks each)

0. NEW-PI-4: `peephole` is now tag `v0.1.4` (Cargo.lock to be refreshed). The calls added are `UnitFacts`
   (all fields public, no `non_exhaustive`), `Loader::from_unset`, `LOADER_VARIABLES`, `peephole::{dirs_trust,
   weaker}` and `ProcFs::{read_file_capped, list_dir, trust_of}`; read from the sibling checkout at 5a2e742. That
   a fixture `FixtureProc::file` overwrites an existing node (tests rely on it).

1. peephole's API was read from the sibling lane's tree at its commit `e3f8ed3`; the dependency is
   the git tag `v0.1.0` (`peephole` and `peephole-core` with feature `desktop`), commit `9cef560`,
   which was not re-read after tagging. The calls are
   confined to `crates/turnstile/src/identity/peephole_authority.rs` and `policy.rs`:
   `Pinned::of_stream`, `Prober::system`, `Prober::probe`, `Prober::fs`, `TableFiles::desktop` and
   `TableFiles::load`, `judge`, `Standing::Service`, `roles::{PAD_DAEMON, KEYMAP_DAEMON}`. The lane
   was still moving; if it changes a signature the change is in those two files. `Pinned::of_stream`
   gives `Pin::AtConnect` from peephole 0.1.5 (`SO_PEERPIDFD` through nix) and `Pin::Reopened` on a
   kernel before 6.5; this service accepts either: it asks for `Proof::Exe`, not for a firm pin.
2. `input-linux` 0.7.1's `UInputHandle::create` calls `UI_DEV_SETUP`, then `UI_ABS_SETUP` for each axis,
   then `UI_DEV_CREATE`; that the kernel accepts the axes after `UI_DEV_SETUP` (it does in
   `drivers/input/misc/uinput.c`, `uinput_abs_setup` only needs the absbit and a device not yet created).
3. The unit's `SystemCallFilter` with the minus-list, `ReadOnlyPaths=/proc` and `CAP_SYS_PTRACE` alone
   is enough to read another user's `/proc/<pid>/exe`: start it in a Fedora, Ubuntu and Arch VM and run
   `systemd-analyze security turnstile.service`.
4. `rustix` 1.1: `recv` returns `(output, real_len)`; `RecvFlags::TRUNC` on `SOCK_SEQPACKET`
   reports the real length; `SO_RCVTIMEO` on an accepted socket ends `recv` with `EAGAIN`.
5. `SO_PEERPIDFD` needs kernel 6.5 (peephole's `Pinned::of_stream`); an older kernel gets the reopened
   pin. `getsockopt` is in `@system-service`, so the unit's `SystemCallFilter` lets it through (read,
   not run under the unit).
6. SELinux on Fedora lets an unconfined service domain open `/dev/uinput` and `/proc/<pid>/exe` of a
   user process.
7. NEW-FA-3: `rustix::net::sockopt::socket_peercred(&OwnedFd)` returns `UCred` with `uid.as_raw()`
   (read in rustix 1.1.5, not compiled); the connection thread's stack is 1 MiB, not 256 KiB, because
   identification now runs on it (not measured); the tests use a `let` chain in an `if` (edition 2024).
8. NEW-FA-4: `input-linux` 0.7.1's `UInputHandle::set_phys(&CStr)` (read, not compiled) and that the kernel keeps
   a phys set before `UI_DEV_SETUP` through `UI_DEV_CREATE` (`uinput_ioctl_handler`, `UI_SET_PHYS`
   only refuses once the device is created); the sysfs file `device/phys` shows it with a newline. The
   manual device test (`device.rs`) checks the real label.

## Deferred

- **Requiring `Pin::AtConnect`.** Since peephole 0.1.6 (`SO_PEERPIDFD` through nix, owner allowed nix
  2026-10-17) a socket peer is pinned at connect on every supported distro's kernel (6.5+), so the
  recycled-pid window is closed there. The service still accepts the reopened pin a kernel before 6.5
  gives (it judges a `Standing` at `Proof::Exe`, not a pin level); refusing it is a one-line
  `PinNeed::AtConnect` check in `policy.rs`, not done until a supported kernel lacks the option.

## Open items

- The units, rule and modules file are placed by the desktop installer (`pa2-desk`); this repository
  only ships them in `dist/`.
- palmrest and keycap switch over (`pa2-palm`, `pa2-key`); their briefs are in the lane report.
