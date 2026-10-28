//! The real authority over a fixture tree: the process behind the socket pair is this test process,
//! the `/proc` and the table are a tree in memory.

use crate::support::{PALMREST_UID, USER};
use peephole::{FixtureImage, FixtureProc, FixtureProcess, Prober, TableFiles};
use peephole_core::{FileTrust, LOADER_VARIABLES, Pid, Proof, RoleName, Uid, Writer};
use rustix::net::sockopt::socket_peercred;
use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::peephole_authority::PeepholeAuthority;
use turnstile::identity::{ActiveUid, Authority, Credential, IdentifyError};
use turnstile::policy::{Refusal, authorise};
use turnstile_wire::Kind;

const MAIN: &str = "/etc/quire/desktop-callers.toml";
const DIR: &str = "/etc/quire/desktop-callers.d";
const TABLE: &str = r#"
[[peer]]
app  = "org.quire.Palmrest"
unit = "palmrest.service"
exe  = ["/usr/local/bin/palmrest"]
role = "pad_daemon"

[[peer]]
app  = "org.quire.Keycap"
unit = "keycap.service"
exe  = ["/usr/local/bin/keycap"]
role = "keymap_daemon"
"#;
const KEYCAP_CGROUP: &str = "0::/system.slice/keycap.service\n";
const KEYCAP_UNIT_FILE: &str = "/usr/lib/systemd/system/keycap.service";

/// The unit's `UnsetEnvironment=` value: every loader variable.
fn unset() -> String {
    LOADER_VARIABLES.join(" ")
}

fn keycap_unit() -> String {
    format!(
        "[Unit]\nDescription=keycap\n[Service]\nExecStart=/usr/local/bin/keycap\nUnsetEnvironment={}\n",
        unset()
    )
}

/// The files of this test's own process: who traces it.
fn proc_files(tree: FixtureProc, tracer: u32) -> FixtureProc {
    let base = format!("/proc/{}", me());
    tree.file(
        &format!("{base}/status"),
        format!("Name:\tx\nTracerPid:\t{tracer}\n"),
        FileTrust::RootOnly,
    )
}

/// The tree of a machine where keycap's unit is the shipped one and its process is clean.
fn keycap_world() -> FixtureProc {
    proc_files(
        tree().file(KEYCAP_UNIT_FILE, keycap_unit(), FileTrust::RootOnly),
        0,
    )
}

/// Fedora's drop-in for every service, under a `lib` folder.
const TIMEOUT_DROP_IN: &str = "systemd/system/service.d/10-timeout-abort.conf";

/// `tree` with `/lib` a link to `target`, as on a merged `/usr`. The tree reads through a link with
/// its target's trust, so a file under `/lib` stands for what the real file system shows there: a
/// writable file (a link's own mode is 0777).
fn merged_usr(tree: FixtureProc, target: &str) -> FixtureProc {
    tree.link("/lib", target)
}

fn socket() -> OwnedFd {
    let (a, _b) = socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket pair");
    a
}

fn me() -> Pid {
    Pid::new(std::process::id()).expect("a process number")
}

fn authority(
    tree: FixtureProc,
    cgroup: &str,
    image: FixtureImage<'_>,
) -> PeepholeAuthority<FixtureProc> {
    let tree = tree.process(FixtureProcess {
        pid: me(),
        cgroup,
        image,
        flatpak_info: None,
    });
    PeepholeAuthority::new(
        Prober::new(tree),
        TableFiles::new(PathBuf::from(MAIN), PathBuf::from(DIR)),
    )
}

fn tree() -> FixtureProc {
    FixtureProc::new()
        .file(MAIN, TABLE, FileTrust::RootOnly)
        .dir(DIR, FileTrust::RootOnly)
}

fn role(text: &str) -> RoleName {
    RoleName::new(text).expect("a role")
}

#[test]
fn keycap_in_its_system_unit_with_a_root_owned_image_and_a_proven_unit_is_listed_at_unit_strength()
{
    let conn = socket();
    let image = FixtureImage::Present("/usr/local/bin/keycap", FileTrust::RootOnly);
    let caller = authority(keycap_world(), KEYCAP_CGROUP, image)
        .identify(&conn)
        .expect("identified");
    assert_eq!(caller.cgroup.as_str(), "/system.slice/keycap.service");
    assert_eq!(
        caller.image.map(|image| image.to_text()).as_deref(),
        Some("/usr/local/bin/keycap")
    );
    assert_eq!(
        caller.uid,
        Uid::new(socket_peercred(&conn).expect("credentials").uid.as_raw())
    );
    assert_eq!(
        caller.credential,
        Some(Credential {
            role: role("keymap_daemon"),
            proof: Proof::Full
        })
    );
}

/// NEW-PI-4: a listed image in the right control group is not enough. Whatever is short of the
/// real unit stops at the image's strength, which the policy refuses.
#[test]
fn a_listed_image_is_unit_strength_only_when_the_unit_and_the_process_both_check_out() {
    let root = FileTrust::RootOnly;
    let mine = FileTrust::Writable(Writer::Owner(Uid::new(1000)));
    let drop_in = "/usr/lib/systemd/system/keycap.service.d/10-local.conf";
    let one_short = LOADER_VARIABLES[1..].join(" ");
    let table: Vec<(&str, FixtureProc, Proof)> = vec![
        ("the real unit", keycap_world(), Proof::Full),
        (
            "a harmless root drop-in",
            keycap_world().file(drop_in, "[Service]\nMemoryMax=1M\n", root),
            Proof::Full,
        ),
        ("no unit file at all", proc_files(tree(), 0), Proof::Exe),
        (
            "a unit file a person can write",
            keycap_world().file(KEYCAP_UNIT_FILE, keycap_unit(), mine),
            Proof::Exe,
        ),
        (
            "a drop-in a person can write",
            keycap_world().file(drop_in, "[Service]\nMemoryMax=1M\n", mine),
            Proof::Exe,
        ),
        (
            "a root drop-in that empties the unset list",
            keycap_world().file(drop_in, "[Service]\nUnsetEnvironment=\n", root),
            Proof::Exe,
        ),
        (
            "a unit that leaves one loader variable out",
            keycap_world().file(
                KEYCAP_UNIT_FILE,
                format!(
                    "[Service]\nExecStart=/usr/local/bin/keycap\nUnsetEnvironment={one_short}\n"
                ),
                root,
            ),
            Proof::Exe,
        ),
        (
            "a unit that runs another program",
            keycap_world().file(
                KEYCAP_UNIT_FILE,
                format!(
                    "[Service]\nExecStart=/usr/bin/other\nUnsetEnvironment={}\n",
                    unset()
                ),
                root,
            ),
            Proof::Exe,
        ),
        (
            "a root drop-in for every service on a merged /usr, seen again through /lib",
            merged_usr(keycap_world(), "/usr/lib")
                .file(&format!("/usr/lib/{TIMEOUT_DROP_IN}"), "[Service]\n", root)
                .file(&format!("/lib/{TIMEOUT_DROP_IN}"), "[Service]\n", mine),
            Proof::Full,
        ),
        (
            "a unit folder linked to a place a person can write",
            merged_usr(keycap_world(), "/home/someone/units")
                .dir("/home/someone/units", mine)
                .file(&format!("/lib/{TIMEOUT_DROP_IN}"), "[Service]\n", mine),
            Proof::Exe,
        ),
        (
            "a run-time unit of that name",
            keycap_world().file("/run/systemd/transient/keycap.service", "", root),
            Proof::Exe,
        ),
        (
            "a process that is being traced",
            proc_files(keycap_world(), 4242),
            Proof::Exe,
        ),
        (
            "a status that cannot be read",
            tree().file(KEYCAP_UNIT_FILE, keycap_unit(), root),
            Proof::Exe,
        ),
    ];
    for (name, tree, want) in table {
        let image = FixtureImage::Present("/usr/local/bin/keycap", root);
        let caller = authority(tree, KEYCAP_CGROUP, image)
            .identify(&socket())
            .expect("identified");
        let proof = match caller.credential {
            Some(Credential { proof, .. }) => proof,
            None => panic!("{name}: unlisted"),
        };
        assert_eq!(proof, want, "{name}");
    }
}

/// NEW-PI-4: a process that passes as palmrest by control group and genuine image, with no unit
/// behind it, is refused; the real system unit is served.
#[test]
fn palmrest_is_served_only_as_its_proven_system_unit() {
    let cgroup = "0::/system.slice/palmrest.service\n";
    let image = || FixtureImage::Present("/usr/local/bin/palmrest", FileTrust::RootOnly);
    let unit = format!(
        "[Service]\nExecStart=/usr/local/bin/palmrest\nUnsetEnvironment={}\n",
        unset()
    );
    let with_unit = || {
        proc_files(
            tree().file(
                "/usr/lib/systemd/system/palmrest.service",
                unit.clone(),
                FileTrust::RootOnly,
            ),
            0,
        )
    };
    let table: Vec<(&str, FixtureProc, Result<(), Refusal>)> = vec![
        ("the real unit", with_unit(), Ok(())),
        (
            "genuine image, forged group, no unit file",
            proc_files(tree(), 0),
            Err(Refusal::TooWeak),
        ),
    ];
    for (name, tree, want) in table {
        let caller = authority(tree, cgroup, image())
            .identify(&socket())
            .expect("identified");
        // The test process stands for palmrest's dynamic user, whatever its real number is.
        let caller = turnstile::identity::Caller {
            uid: PALMREST_UID,
            ..caller
        };
        assert_eq!(
            authorise(Kind::PalmrestKeys, &caller, ActiveUid(Some(USER))),
            want,
            "{name}"
        );
    }
}

#[test]
fn a_hidden_image_is_only_the_cgroup_strength_and_the_policy_refuses_it() {
    let conn = socket();
    let cgroup = "0::/system.slice/palmrest.service\n";
    let caller = authority(tree(), cgroup, FixtureImage::Hidden)
        .identify(&conn)
        .expect("identified");
    assert_eq!(
        caller.credential,
        Some(Credential {
            role: role("pad_daemon"),
            proof: Proof::Cgroup
        })
    );
    // Even in the right place and as the right user, a caller whose image was not read gets nothing.
    let caller = turnstile::identity::Caller {
        uid: PALMREST_UID,
        ..caller
    };
    assert_eq!(
        authorise(Kind::PalmrestWheel, &caller, ActiveUid(Some(USER))),
        Err(Refusal::TooWeak)
    );
}

#[test]
fn a_user_writable_image_a_stranger_and_a_wrong_row_are_all_unlisted() {
    let writable = FixtureImage::Present(
        "/usr/local/bin/keycap",
        FileTrust::Writable(Writer::Owner(Uid::new(1000))),
    );
    let stranger = FixtureImage::Present("/usr/bin/vim", FileTrust::RootOnly);
    let wrong_image = FixtureImage::Present("/tmp/keycap", FileTrust::RootOnly);
    for (name, image) in [
        ("writable", writable),
        ("stranger", stranger),
        ("another path", wrong_image),
    ] {
        let caller = authority(tree(), KEYCAP_CGROUP, image)
            .identify(&socket())
            .expect("identified");
        assert_eq!(caller.credential, None, "{name}");
    }
}

#[test]
fn without_a_table_nobody_is_identified() {
    let tree = FixtureProc::new();
    let image = FixtureImage::Present("/usr/local/bin/keycap", FileTrust::RootOnly);
    let err = authority(tree, KEYCAP_CGROUP, image)
        .identify(&socket())
        .expect_err("no table");
    assert!(matches!(err, IdentifyError::Table(_)), "{err:?}");
}

#[test]
fn a_table_a_user_can_write_is_not_believed() {
    let tree = FixtureProc::new()
        .file(
            MAIN,
            TABLE,
            FileTrust::Writable(Writer::Owner(Uid::new(1000))),
        )
        .dir(DIR, FileTrust::RootOnly);
    let image = FixtureImage::Present("/usr/local/bin/keycap", FileTrust::RootOnly);
    let err = authority(tree, KEYCAP_CGROUP, image)
        .identify(&socket())
        .expect_err("untrusted table");
    assert!(matches!(err, IdentifyError::Table(_)), "{err:?}");
}

#[test]
fn a_peer_the_probe_cannot_find_is_a_failure_not_a_guess() {
    // The fixture has no process for this test's pid.
    let authority = PeepholeAuthority::new(
        Prober::new(tree()),
        TableFiles::new(PathBuf::from(MAIN), PathBuf::from(DIR)),
    );
    assert!(authority.identify(&socket()).is_err());
    // And the fixed authority used by the other tests fails the same way when told to.
    assert!(FixedAuthority::failing().identify(&socket()).is_err());
}
