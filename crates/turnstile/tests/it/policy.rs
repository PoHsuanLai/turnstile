use crate::support::{KEYCAP_UID, PALMREST_CGROUP, PALMREST_UID, USER, keycap, listed, palmrest};
use turnstile::identity::{ActiveUid, Caller, Credential, Strength};
use turnstile::policy::{Refusal, authorise};
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
            authorise(kind, &palmrest(), seat(Some(USER))),
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
            authorise(kind, &palmrest(), seat(Some(USER))).is_err(),
            "palmrest asking for {kind:?}"
        );
    }
}

#[test]
fn both_are_served_before_anyone_logs_in() {
    assert_eq!(
        authorise(Kind::KeycapKeyboard, &keycap(), seat(None)),
        Ok(())
    );
    assert_eq!(
        authorise(Kind::PalmrestWheel, &palmrest(), seat(None)),
        Ok(())
    );
}

#[test]
fn refusals_table() {
    use Refusal::*;
    let user_palmrest = "/user.slice/user-1000.slice/user@1000.service/app.slice/palmrest.service";
    let user_keycap = "/user.slice/user-1000.slice/user@1000.service/app.slice/keycap.service";
    let table: Vec<(&str, Kind, Caller, ActiveUid, Refusal)> = vec![
        (
            "not in the table",
            Kind::PalmrestWheel,
            Caller {
                credential: Credential::Unlisted,
                ..palmrest()
            },
            seat(Some(USER)),
            Unlisted,
        ),
        (
            "another role: the shell",
            Kind::PalmrestWheel,
            listed("shell", Strength::Unit, PALMREST_UID, PALMREST_CGROUP),
            seat(Some(USER)),
            WrongRole {
                have: "shell".to_owned(),
            },
        ),
        (
            "palmrest asks for keycap's keyboard",
            Kind::KeycapKeyboard,
            palmrest(),
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
            listed(
                "pad_daemon",
                Strength::Cgroup,
                PALMREST_UID,
                PALMREST_CGROUP,
            ),
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
            "palmrest running as the logged in user",
            Kind::PalmrestWheel,
            listed("pad_daemon", Strength::Unit, USER, PALMREST_CGROUP),
            seat(Some(USER)),
            WrongUser,
        ),
        (
            "palmrest as a user unit of the person",
            Kind::PalmrestWheel,
            listed("pad_daemon", Strength::Unit, PALMREST_UID, user_palmrest),
            seat(Some(USER)),
            WrongPlace,
        ),
        (
            "palmrest in a lookalike system unit",
            Kind::PalmrestWheel,
            listed(
                "pad_daemon",
                Strength::Unit,
                PALMREST_UID,
                "/system.slice/palmrest.service/x",
            ),
            seat(Some(USER)),
            WrongPlace,
        ),
        (
            "palmrest in keycap's unit",
            Kind::PalmrestWheel,
            listed(
                "pad_daemon",
                Strength::Unit,
                PALMREST_UID,
                "/system.slice/keycap.service",
            ),
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
        let pad = listed("pad_daemon", *strength, PALMREST_UID, PALMREST_CGROUP);
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
