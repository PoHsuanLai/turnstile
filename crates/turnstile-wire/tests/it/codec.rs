use turnstile_wire::limits::{
    MAX_DATAGRAM, MAX_EVENTS_PER_DATAGRAM, MAX_FEEDBACK_EVENTS, MAX_KEY_CODES, PROTOCOL_VERSION,
};
use turnstile_wire::{
    AbsAxis, Create, Delivery, DeviceId, Event, PointerCaps, Reason, Reply, Request, WireError,
    decode_reply, decode_request, encode_reply, encode_request,
};

fn pointer() -> PointerCaps {
    PointerCaps {
        label: "Apple Magic Trackpad".to_owned(),
        vendor: 0x05ac,
        product: 0x0265,
        props: vec![0, 2],
        buttons: vec![0x110, 0x14a],
        rels: vec![0, 1, 8],
        abs: vec![AbsAxis {
            code: 0x35,
            minimum: -100,
            maximum: 6000,
            fuzz: 3,
            flat: 0,
            resolution: 12,
        }],
    }
}

fn requests() -> Vec<Request> {
    let events = |n: usize| {
        (0..n)
            .map(|i| Event::new(3, 0x35, i as i32 - 5))
            .collect::<Vec<_>>()
    };
    vec![
        Request::Create(Create::touchpad()),
        Request::Create(Create::wheel()),
        Request::Create(Create::keys(vec![29, 56, 105, 106])),
        Request::Create(Create::keys(vec![1; MAX_KEY_CODES])),
        Request::Create(Create::keyboard()),
        Request::Create(Create::pointer(pointer())),
        Request::Create(Create::pointer(PointerCaps::default())),
        Request::Events {
            device: DeviceId(2),
            events: events(1),
        },
        Request::Events {
            device: DeviceId(255),
            events: events(MAX_EVENTS_PER_DATAGRAM),
        },
        Request::Destroy {
            device: DeviceId(7),
        },
    ]
}

fn replies() -> Vec<Reply> {
    vec![
        Reply::Created {
            device: DeviceId(0),
            delivery: Delivery::Proxied,
        },
        Reply::Created {
            device: DeviceId(3),
            delivery: Delivery::Handed,
        },
        Reply::Feedback {
            device: DeviceId(1),
            events: vec![Event::new(0x11, 1, 1); MAX_FEEDBACK_EVENTS],
        },
        Reply::Refused(Reason::NotAllowed),
        Reply::Refused(Reason::TooMany),
        Reply::Refused(Reason::BadCaps),
        Reply::Refused(Reason::Unavailable),
        Reply::Refused(Reason::Unknown),
    ]
}

#[test]
fn every_request_round_trips_within_one_datagram() {
    for request in requests() {
        let bytes = encode_request(&request).expect("encodes");
        assert!(bytes.len() <= MAX_DATAGRAM, "{request:?}");
        assert_eq!(bytes[0], PROTOCOL_VERSION);
        assert_eq!(decode_request(&bytes).as_ref(), Ok(&request));
    }
}

#[test]
fn every_reply_round_trips() {
    for reply in replies() {
        let bytes = encode_reply(&reply).expect("encodes");
        assert_eq!(decode_reply(&bytes).as_ref(), Ok(&reply));
    }
}

#[test]
fn every_proper_prefix_of_a_datagram_is_an_error_not_a_panic() {
    for request in requests() {
        let bytes = encode_request(&request).expect("encodes");
        for cut in 0..bytes.len() {
            // Events fill the rest of the datagram, so a prefix cut on an event boundary is a
            // shorter valid message; everything else must fail.
            let got = decode_request(&bytes[..cut]);
            if let Ok(Request::Events { .. }) = got {
                continue;
            }
            assert!(got.is_err(), "{request:?} cut at {cut}");
        }
    }
    for reply in replies() {
        let bytes = encode_reply(&reply).expect("encodes");
        for cut in 0..bytes.len() {
            assert!(
                decode_reply(&bytes[..cut]).is_err(),
                "{reply:?} cut at {cut}"
            );
        }
    }
}

#[test]
fn malformed_requests_are_named_errors() {
    let v = PROTOCOL_VERSION;
    let cases: Vec<(&str, Vec<u8>, WireError)> = vec![
        ("empty", vec![], WireError::Empty),
        ("version", vec![9, 1, 1], WireError::Version(9)),
        ("tag", vec![v, 77], WireError::Tag(77)),
        ("kind", vec![v, 1, 99], WireError::Kind(99)),
        (
            "trailing after touchpad",
            vec![v, 1, 1, 0],
            WireError::Trailing(1),
        ),
        (
            "trailing after destroy",
            vec![v, 3, 0, 0],
            WireError::Trailing(1),
        ),
        (
            "keys count over the cap",
            vec![v, 1, 3, 65],
            WireError::TooMany {
                what: "the key list",
                got: 65,
                max: MAX_KEY_CODES,
            },
        ),
        ("events with none", vec![v, 2, 0], WireError::Truncated),
        (
            "events cut mid event",
            {
                let mut b = vec![v, 2, 0];
                b.extend([0u8; 23]);
                b
            },
            WireError::Truncated,
        ),
        (
            "events over the cap",
            {
                let mut b = vec![v, 2, 0];
                b.extend(vec![0u8; 24 * (MAX_EVENTS_PER_DATAGRAM + 1)]);
                b
            },
            WireError::TooMany {
                what: "the event list",
                got: MAX_EVENTS_PER_DATAGRAM + 1,
                max: MAX_EVENTS_PER_DATAGRAM,
            },
        ),
    ];
    for (name, bytes, want) in cases {
        assert_eq!(decode_request(&bytes), Err(want), "{name}");
    }
}

#[test]
fn a_pointer_label_must_be_short_plain_text() {
    let v = PROTOCOL_VERSION;
    // kind 17, then a label of the given bytes, then the rest of an empty pointer.
    let with_label = |label: &[u8]| {
        let mut b = vec![v, 1, 17, label.len() as u8];
        b.extend(label);
        b.extend([0, 0, 0, 0, 0, 0, 0, 0]);
        b
    };
    assert!(decode_request(&with_label(b"ok")).is_ok());
    assert_eq!(
        decode_request(&with_label(&[0xff, 0xfe])),
        Err(WireError::Label)
    );
    assert_eq!(
        decode_request(&with_label(b"tab\there")),
        Err(WireError::Label)
    );
    assert_eq!(
        decode_request(&with_label(&[b'a'; 63])),
        Err(WireError::Label)
    );
    assert!(decode_request(&with_label(&[b'a'; 62])).is_ok());
}

#[test]
fn encoding_refuses_what_decoding_would() {
    let long_label = PointerCaps {
        label: "x".repeat(63),
        ..PointerCaps::default()
    };
    let too_many_buttons = PointerCaps {
        buttons: vec![0x110; 17],
        ..PointerCaps::default()
    };
    let bad: Vec<Request> = vec![
        Request::Create(Create::keys(vec![1; MAX_KEY_CODES + 1])),
        Request::Create(Create::pointer(long_label)),
        Request::Create(Create::pointer(too_many_buttons)),
        Request::Events {
            device: DeviceId(0),
            events: vec![],
        },
        Request::Events {
            device: DeviceId(0),
            events: vec![Event::SYNC; MAX_EVENTS_PER_DATAGRAM + 1],
        },
    ];
    for request in bad {
        assert!(encode_request(&request).is_err(), "{request:?}");
    }
}

#[test]
fn a_reason_from_a_newer_service_reads_as_unknown() {
    let bytes = [PROTOCOL_VERSION, 3, 200];
    assert_eq!(decode_reply(&bytes), Ok(Reply::Refused(Reason::Unknown)));
}

#[test]
fn random_bytes_never_panic_either_decoder() {
    // xorshift64: deterministic, no dependency.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..20_000 {
        let len = (next() % 200) as usize;
        let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        // Half the inputs start like a real message so the bodies are reached.
        if len >= 2 && next() % 2 == 0 {
            bytes[0] = PROTOCOL_VERSION;
            bytes[1] = (next() % 4) as u8;
        }
        let _ = decode_request(&bytes);
        let _ = decode_reply(&bytes);
    }
}
