use turnstile_client::fake::{FakeSource, OpenMode, SendMode};
use turnstile_client::{
    ClientError, Create, DeviceId, DeviceSource, Event, Made, Reason, Sent, Session,
};

#[test]
fn the_fake_records_and_applies_the_real_ceiling() {
    let source = FakeSource::new();
    let mut session = source.open().expect("opens");
    let pad = session.create(&Create::touchpad()).expect("created");
    assert!(matches!(pad, Made::Proxied(DeviceId(0))));
    // The ceiling says no keys outside the table, as the service would.
    let err = session
        .create(&Create::keys(vec![9999]))
        .expect_err("outside the ceiling");
    assert!(
        matches!(err, ClientError::Refused(Reason::BadCaps)),
        "{err:?}"
    );
    assert_eq!(
        session.send(DeviceId(0), &[Event::SYNC]).expect("sent"),
        Sent::Queued
    );
    session.destroy(DeviceId(0)).expect("destroyed");
    let state = source.state();
    assert_eq!(state.opened, 1);
    assert_eq!(state.created.len(), 2);
    assert_eq!(state.sent, [(DeviceId(0), vec![Event::SYNC])]);
    assert_eq!(state.destroyed, [DeviceId(0)]);
}

#[test]
fn the_fake_can_be_told_to_refuse_drop_close_or_be_absent() {
    let source = FakeSource::new();
    let mut session = source.open().expect("opens");
    source.state().refuse_next = Some(Reason::NotAllowed);
    let err = session.create(&Create::wheel()).expect_err("refused");
    assert!(
        matches!(err, ClientError::Refused(Reason::NotAllowed)),
        "{err:?}"
    );
    assert!(
        session.create(&Create::wheel()).is_ok(),
        "only the next one"
    );

    source.state().send_mode = SendMode::Drop;
    assert_eq!(
        session.send(DeviceId(0), &[Event::SYNC]).expect("sent"),
        Sent::Dropped
    );
    source.state().send_mode = SendMode::Close;
    assert!(matches!(
        session.send(DeviceId(0), &[Event::SYNC]),
        Err(ClientError::Closed)
    ));

    source.state().open_mode = OpenMode::Absent;
    assert!(matches!(source.open(), Err(ClientError::Absent)));
}

#[test]
fn a_handed_device_in_the_fake_is_a_descriptor_whose_other_end_the_test_holds() {
    let source = FakeSource::new();
    let mut session = source.open().expect("opens");
    let Made::Handed { fd, .. } = session.create(&Create::keyboard()).expect("created") else {
        panic!("handed");
    };
    rustix::net::send(&fd, b"x", rustix::net::SendFlags::NOSIGNAL).expect("write");
    let state = source.state();
    let mut buf = [0u8; 4];
    let (n, _) = rustix::net::recv(
        &state.handed_peers[0],
        &mut buf[..],
        rustix::net::RecvFlags::empty(),
    )
    .expect("read");
    assert_eq!(&buf[..n], b"x");
}
