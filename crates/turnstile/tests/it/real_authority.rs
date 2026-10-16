//! The real authority over a fixture tree: the process behind the socket pair is this test process,
//! the `/proc` and the table are a tree in memory.

use crate::support::USER;
use peephole::{FixtureImage, FixtureProc, FixtureProcess, Prober, TableFiles};
use peephole_core::{FileTrust, Pid, RoleName, Uid, Writer};
use rustix::net::sockopt::socket_peercred;
use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use turnstile::identity::fake::FixedAuthority;
use turnstile::identity::peephole_authority::PeepholeAuthority;
use turnstile::identity::{ActiveUid, Authority, Credential, IdentifyError, Strength};
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
fn keycap_in_its_system_unit_with_a_root_owned_image_is_listed_at_image_strength() {
    let conn = socket();
    let image = FixtureImage::Present("/usr/local/bin/keycap", FileTrust::RootOnly);
    let caller = authority(tree(), KEYCAP_CGROUP, image)
        .identify(&conn)
        .expect("identified");
    assert_eq!(caller.cgroup, "/system.slice/keycap.service");
    assert_eq!(caller.image.as_deref(), Some("/usr/local/bin/keycap"));
    assert_eq!(
        caller.uid,
        socket_peercred(&conn).expect("credentials").uid.as_raw()
    );
    assert_eq!(
        caller.credential,
        Credential::Listed {
            role: role("keymap_daemon"),
            strength: Strength::Image
        }
    );
}

#[test]
fn a_hidden_image_is_only_the_cgroup_strength_and_the_policy_refuses_it() {
    let conn = socket();
    let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/palmrest.service\n";
    let caller = authority(tree(), cgroup, FixtureImage::Hidden)
        .identify(&conn)
        .expect("identified");
    assert_eq!(
        caller.credential,
        Credential::Listed {
            role: role("pad_daemon"),
            strength: Strength::Cgroup
        }
    );
    // Even in the right place and as the right user, a caller whose image was not read gets nothing.
    let caller = turnstile::identity::Caller {
        uid: USER,
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
        assert_eq!(caller.credential, Credential::Unlisted, "{name}");
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
    assert!(FixedAuthority::failing("x").identify(&socket()).is_err());
}
