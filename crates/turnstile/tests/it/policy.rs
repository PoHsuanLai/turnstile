use crate::support::{KEYCAP_UID, USER, keycap, listed, palmrest, palmrest_cgroup};
use turnstile::identity::{ActiveUid, Caller, Credential, Strength};
use turnstile::policy::{Refusal, authorise, in_user_unit};
use turnstile_wire::Kind;

const PALMREST_KINDS: [Kind; 3] = [
    Kind::PalmrestTouchpad,
    Kind::PalmrestWheel,
    Kind::PalmrestKeys,
];
const KEYCAP_KINDS: [Kind; 2] = [Kind::KeycapKeyboard, Kind::KeycapPointer];

fn seat(uid: Option<u32>) -> ActiveUid {
    ActiveUid(uid)
}

#[test]
fn the_genuine_programs_get_their_own_devices_and_only_those() {
    for kind in PALMREST_KINDS {
        assert_eq!(
            authorise(kind, &palmrest(USER), seat(Some(USER))),
            Ok(()),
            "{kind:?}"
        );
        assert!(
            authorise(kind, &keycap(), seat(Some(USER))).is_err(),
            "keycap asking for {kind:?}"
        );
    }
    for kind in KEYCAP_KINDS {
        assert_eq!(
            authorise(kind, &keycap(), seat(Some(USER))),
            Ok(()),
            "{kind:?}"
        );
        assert!(
            authorise(kind, &palmrest(USER), seat(Some(USER))).is_err(),
            "palmrest asking for {kind:?}"
        );
    }
}

#[test]
fn keycap_is_served_before_anyone_logs_in() {
    assert_eq!(
        authorise(Kind::KeycapKeyboard, &keycap(), seat(None)),
        Ok(())
    );
    assert_eq!(
        authorise(Kind::PalmrestWheel, &palmrest(USER), seat(None)),
        Err(Refusal::NoSeat)
    );
}

#[test]
fn refusals_table() {
    use Refusal::*;
    let other_user_cgroup = palmrest_cgroup(1001);
    let system_palmrest = "/system.slice/palmrest.service";
    let user_keycap = "/user.slice/user-1000.slice/user@1000.service/app.slice/keycap.service";
    let table: Vec<(&str, Kind, Caller, ActiveUid, Refusal)> = vec![
        (
            "not in the table",
            Kind::PalmrestWheel,
            Caller {
                credential: Credential::Unlisted,
                ..palmrest(USER)
            },
            seat(Some(USER)),
            Unlisted,
        ),
        (
            "another role: the shell",
            Kind::PalmrestWheel,
            listed("shell", Strength::Unit, USER, &palmrest_cgroup(USER)),
            seat(Some(USER)),
            WrongRole {
                have: "shell".to_owned(),
            },
        ),
        (
            "palmrest asks for keycap's keyboard",
            Kind::KeycapKeyboard,
            palmrest(USER),
            seat(Some(USER)),
            WrongRole {
                have: "pad_daemon".to_owned(),
            },
        ),
        (
            "keycap asks for palmrest's keys",
            Kind::PalmrestKeys,
            keycap(),
            seat(Some(USER)),
            WrongRole {
                have: "keymap_daemon".to_owned(),
            },
        ),
        (
            "palmrest proven by its control group only",
            Kind::PalmrestTouchpad,
            listed("pad_daemon", Strength::Cgroup, USER, &palmrest_cgroup(USER)),
            seat(Some(USER)),
            TooWeak,
        ),
        (
            "keycap proven by its control group only",
            Kind::KeycapKeyboard,
            listed(
                "keymap_daemon",
                Strength::Cgroup,
                KEYCAP_UID,
                "/system.slice/keycap.service",
            ),
            seat(Some(USER)),
            TooWeak,
        ),
        (
            "palmrest of a user who is not at the seat",
            Kind::PalmrestWheel,
            palmrest(1001),
            seat(Some(USER)),
            WrongUser,
        ),
        (
            "palmrest in another user's manager",
            Kind::PalmrestWheel,
            listed("pad_daemon", Strength::Unit, USER, &other_user_cgroup),
            seat(Some(USER)),
            WrongPlace,
        ),
        (
            "palmrest as a system service",
            Kind::PalmrestWheel,
            listed("pad_daemon", Strength::Unit, USER, system_palmrest),
            seat(Some(USER)),
            WrongPlace,
        ),
        (
            "keycap running as the logged in user",
            Kind::KeycapKeyboard,
            listed(
                "keymap_daemon",
                Strength::Unit,
                USER,
                "/system.slice/keycap.service",
            ),
            seat(Some(USER)),
            WrongUser,
        ),
        (
            "keycap as a user unit of the person",
            Kind::KeycapKeyboard,
            listed("keymap_daemon", Strength::Unit, KEYCAP_UID, user_keycap),
            seat(Some(USER)),
            WrongPlace,
        ),
        (
            "keycap in a lookalike system unit",
            Kind::KeycapPointer,
            listed(
                "keymap_daemon",
                Strength::Unit,
                KEYCAP_UID,
                "/system.slice/keycap.service/x",
            ),
            seat(Some(USER)),
            WrongPlace,
        ),
    ];
    for (name, kind, caller, active, want) in table {
        assert_eq!(authorise(kind, &caller, active), Err(want), "{name}");
    }
}

/// NEW-PI-4: every strength below the unit's own is refused for every kind, in the right place and
/// as the right user; only the unit's proof is enough.
#[test]
fn only_a_proven_unit_gets_devices_whatever_the_place_is() {
    let palmrest_at = palmrest_cgroup(USER);
    let table: &[(&str, Strength, Result<(), Refusal>)] = &[
        (
            "its control group only",
            Strength::Cgroup,
            Err(Refusal::TooWeak),
        ),
        (
            "its control group and its image",
            Strength::Image,
            Err(Refusal::TooWeak),
        ),
        ("its unit", Strength::Unit, Ok(())),
    ];
    for (name, strength, want) in table {
        let pad = listed("pad_daemon", *strength, USER, &palmrest_at);
        let key = listed(
            "keymap_daemon",
            *strength,
            KEYCAP_UID,
            "/system.slice/keycap.service",
        );
        for kind in PALMREST_KINDS {
            assert_eq!(
                authorise(kind, &pad, seat(Some(USER))),
                *want,
                "palmrest, {name}, {kind:?}"
            );
        }
        for kind in KEYCAP_KINDS {
            assert_eq!(
                authorise(kind, &key, seat(Some(USER))),
                *want,
                "keycap, {name}, {kind:?}"
            );
        }
    }
}

#[test]
fn user_unit_paths() {
    let root = "/user.slice/user-1000.slice/user@1000.service";
    let table: &[(&str, String, bool)] = &[
        (
            "under app.slice",
            format!("{root}/app.slice/palmrest.service"),
            true,
        ),
        (
            "directly under the manager",
            format!("{root}/palmrest.service"),
            true,
        ),
        (
            "deeper",
            format!("{root}/app.slice/x.slice/palmrest.service"),
            true,
        ),
        (
            "another unit",
            format!("{root}/app.slice/other.service"),
            false,
        ),
        (
            "a longer name",
            format!("{root}/app.slice/palmrest.service.d"),
            false,
        ),
        (
            "another user",
            "/user.slice/user-1001.slice/user@1001.service/app.slice/palmrest.service".to_owned(),
            false,
        ),
        (
            "a scope inside the unit",
            format!("{root}/app.slice/palmrest.service/x.scope"),
            false,
        ),
        ("no leaf", format!("{root}/app.slice/"), false),
        (
            "the system",
            "/system.slice/palmrest.service".to_owned(),
            false,
        ),
        ("empty", String::new(), false),
    ];
    for (name, cgroup, want) in table {
        assert_eq!(
            in_user_unit(cgroup, 1000, "palmrest.service"),
            *want,
            "{name}"
        );
    }
}
