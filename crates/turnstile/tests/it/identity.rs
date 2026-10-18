use peephole_core::{CgroupPath, Pid, Uid};
use turnstile::identity::seat::{SeatFile, SeatSource, parse_active_uid};
use turnstile::identity::{ActiveUid, Caller, Credential};

#[test]
fn the_active_user_is_the_active_uid_line() {
    let table: &[(&str, &str, Option<u32>)] = &[
        (
            "logged in",
            "IS_SEAT0=1\nACTIVE=1\nACTIVE_SESSION=2\nACTIVE_UID=1000\nUIDS=1000\n",
            Some(1000),
        ),
        ("nobody yet", "IS_SEAT0=1\nCAN_GRAPHICAL=1\n", None),
        ("empty", "", None),
        ("not a number", "ACTIVE_UID=root\n", None),
        ("padded", "ACTIVE_UID= 42 \n", Some(42)),
        ("a lookalike key", "XACTIVE_UID=7\n", None),
        (
            "the first one wins",
            "ACTIVE_UID=1\nACTIVE_UID=2\n",
            Some(1),
        ),
    ];
    for (name, text, want) in table {
        assert_eq!(
            parse_active_uid(text),
            ActiveUid((*want).map(Uid::new)),
            "{name}"
        );
    }
}

#[test]
fn the_seat_file_is_read_each_time_and_a_missing_one_means_no_one() {
    let dir = tempfile::tempdir().expect("scratch folder");
    let path = dir.path().join("seat0");
    let seat = SeatFile::at(&path);
    assert_eq!(seat.active_uid(), ActiveUid(None));
    std::fs::write(&path, "ACTIVE_UID=1000\n").expect("write");
    assert_eq!(seat.active_uid(), ActiveUid(Some(Uid::new(1000))));
    std::fs::write(&path, "ACTIVE_UID=1001\n").expect("write");
    assert_eq!(seat.active_uid(), ActiveUid(Some(Uid::new(1001))));
}

#[test]
fn a_caller_is_described_for_the_log_without_trusting_it() {
    let caller = Caller {
        pid: Pid::new(7).expect("a process number"),
        uid: Uid::new(1000),
        cgroup: CgroupPath::new("/").expect("a cgroup path"),
        image: None,
        credential: Credential::Unlisted,
    };
    assert_eq!(caller.describe(), "pid=7 uid=1000 image=?");
}
