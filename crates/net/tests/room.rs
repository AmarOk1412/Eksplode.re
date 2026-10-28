#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use eksplode_re_core::{BoardSize, Mode, RoomSettings};
use eksplode_re_net::room::from_wire;
use serde_json::json;

#[test]
fn service_identity_and_proofs_use_injected_randomness_without_exposing_secrets() {
    use eksplode_re_net::room::{RandomSource, RejoinProof, RoomError, ServerIdentity};

    struct Bytes(u8);
    impl RandomSource for Bytes {
        fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
            output.fill(self.0);
            Ok(())
        }
    }
    struct Failed;
    impl RandomSource for Failed {
        fn fill(&self, _: &mut [u8]) -> Result<(), RoomError> {
            Err(RoomError::RandomUnavailable)
        }
    }
    let identity = ServerIdentity::new(Some("alpha"), &Bytes(0xab)).unwrap();
    assert_eq!(identity.server_id, "alpha");
    assert_eq!(identity.server_epoch, "ab".repeat(16));
    assert!(ServerIdentity::new(None, &Failed).is_err());
    for invalid in ["", "with space", "nonascii-é", &"x".repeat(65)] {
        assert!(ServerIdentity::new(Some(invalid), &Bytes(1)).is_err());
    }

    let proof = RejoinProof::generate(&Bytes(0xcd)).unwrap();
    assert_eq!(proof.to_secret().expose(), "cd".repeat(32));
    assert!(proof.matches(&proof.to_secret()));
    assert!(!proof.matches(&eksplode_re_net::protocol::v2::Secret::new("ef".repeat(32))));
    assert!(!proof.matches(&eksplode_re_net::protocol::v2::Secret::new("not a proof")));
    assert!(!format!("{proof:?}").contains(&"cd".repeat(32)));
    assert!(RejoinProof::generate(&Failed).is_err());
}

#[test]
fn service_limits_and_clock_keep_network_policy_outside_core() {
    use eksplode_re_net::room::{RoomLimits, ServiceClock};
    use std::time::Duration;

    let defaults = RoomLimits::default();
    assert_eq!((defaults.max_rooms, defaults.max_connections), (32, 256));
    assert!(defaults.validate().is_ok());
    for (rooms, connections) in [(0, 256), (129, 256), (32, 3), (32, 1025)] {
        assert!(RoomLimits {
            max_rooms: rooms,
            max_connections: connections
        }
        .validate()
        .is_err());
    }
    for (rooms, connections) in [(1, 4), (128, 1024)] {
        assert!(RoomLimits {
            max_rooms: rooms,
            max_connections: connections
        }
        .validate()
        .is_ok());
    }
    struct Clock(common::FakeClock);
    impl ServiceClock for Clock {
        fn now(&self) -> Duration {
            self.0.now()
        }
    }
    let clock = Clock(common::FakeClock::default());
    clock.0.advance(100);
    assert_eq!(ServiceClock::now(&clock), Duration::from_millis(100));
    let settings = serde_json::to_value(RoomSettings::default()).unwrap();
    for private in [
        "room_id",
        "proof",
        "password",
        "reserved_until",
        "owner_membership_id",
    ] {
        assert!(settings.get(private).is_none());
    }
}

#[test]
fn admission_ledger_bounds_deadlines_duplicates_and_cancellation() {
    use eksplode_re_net::protocol::v2::{ErrorCode, Reply, RequestId, RequestState};
    use eksplode_re_net::room::AdmissionLedger;
    use std::time::Duration;

    let mut ledger = AdmissionLedger::default();
    let first = RequestId::new("first").unwrap();
    let second = RequestId::new("second").unwrap();
    let start = Duration::ZERO;
    assert_eq!(
        ledger.begin(first.clone(), start).unwrap(),
        RequestState::Pending
    );
    assert_eq!(
        ledger.begin(first.clone(), start).unwrap(),
        RequestState::Pending
    );
    assert_eq!(ledger.begin(second.clone(), start), Err(ErrorCode::Busy));
    assert_eq!(
        ledger.status(&first, Duration::from_secs(5)).0,
        RequestState::Expired
    );
    assert_eq!(
        ledger.finish(
            &first,
            Duration::from_secs(5),
            Reply::Pong { nonce: 1 },
            true
        ),
        Err(ErrorCode::RequestExpired)
    );
    assert_eq!(
        ledger
            .begin(second.clone(), Duration::from_secs(5))
            .unwrap(),
        RequestState::Pending
    );
    assert_eq!(
        ledger.cancel(&second, Duration::from_secs(6)).0,
        RequestState::Cancelled
    );
    assert_eq!(
        ledger.finish(
            &second,
            Duration::from_secs(6),
            Reply::Pong { nonce: 2 },
            true
        ),
        Err(ErrorCode::Cancelled)
    );
    let committed = RequestId::new("committed").unwrap();
    ledger
        .begin(committed.clone(), Duration::from_secs(7))
        .unwrap();
    ledger
        .finish(
            &committed,
            Duration::from_secs(8),
            Reply::Pong { nonce: 3 },
            true,
        )
        .unwrap();
    assert_eq!(
        ledger.cancel(&committed, Duration::from_secs(9)),
        (RequestState::Committed, Some(Reply::Pong { nonce: 3 }))
    );
    assert_eq!(
        ledger.status(&committed, Duration::from_secs(68)).0,
        RequestState::Unknown
    );
}

#[test]
fn admission_ledger_keeps_only_eight_unexpired_results() {
    use eksplode_re_net::protocol::v2::{ErrorCode, Reply, RequestId};
    use eksplode_re_net::room::AdmissionLedger;
    use std::time::Duration;

    let mut ledger = AdmissionLedger::default();
    for index in 0..8 {
        let request_id = RequestId::new(format!("request-{index}")).unwrap();
        ledger.begin(request_id.clone(), Duration::ZERO).unwrap();
        ledger
            .finish(
                &request_id,
                Duration::ZERO,
                Reply::Pong { nonce: index },
                false,
            )
            .unwrap();
    }
    let extra = RequestId::new("extra").unwrap();
    assert_eq!(
        ledger.begin(extra.clone(), Duration::ZERO),
        Err(ErrorCode::Busy)
    );
    assert!(ledger.begin(extra, Duration::from_secs(60)).is_ok());
}

#[test]
fn host_uses_injected_service_time_and_randomness() {
    use eksplode_re_net::events::{EventKind, EventRecord, EventSink};
    use eksplode_re_net::host::HostServices;
    use eksplode_re_net::protocol::{v2, ClientMsg, ServerMsg};
    use eksplode_re_net::room::{RandomSource, RoomError, ServiceClock};
    use eksplode_re_net::{Host, HostConfig};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct Clock(common::FakeClock);
    impl ServiceClock for Clock {
        fn now(&self) -> Duration {
            self.0.now()
        }
        fn timestamp_ms(&self) -> u64 {
            42_000
        }
    }
    struct Random;
    impl RandomSource for Random {
        fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
            output.fill(0xab);
            Ok(())
        }
    }
    struct Capture(Mutex<Vec<EventRecord>>);
    impl EventSink for Capture {
        fn try_emit(&self, event: EventRecord) -> bool {
            self.0.lock().unwrap().push(event);
            true
        }
    }
    let clock = common::FakeClock::default();
    let events = Arc::new(Capture(Mutex::new(Vec::new())));
    let host = Host::start_with_services(
        HostConfig {
            bind: "127.0.0.1:0".into(),
            ..HostConfig::default()
        },
        HostServices::new(Arc::new(Clock(clock.clone())), Arc::new(Random))
            .with_events(events.clone()),
    )
    .unwrap();
    let silent = host.connect(false).unwrap();
    let observer = host.connect(false).unwrap();
    observer
        .send(ClientMsg::RoomV2(Box::new(v2::Request::Hello {
            version: 2,
        })))
        .unwrap();
    match observer.recv_timeout(common::WAIT).unwrap() {
        ServerMsg::RoomV2(reply) => match *reply {
            v2::Reply::Hello { server_epoch, .. } => assert_eq!(server_epoch, "ab".repeat(16)),
            other => panic!("expected Hello, got {other:?}"),
        },
        other => panic!("expected v2 response, got {other:?}"),
    }
    clock.advance(2_000);
    assert!(matches!(
        silent.recv_timeout(common::WAIT),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    ));
    drop(host);
    let events = events.0.lock().unwrap();
    let server_events: Vec<EventKind> = events
        .iter()
        .filter_map(|event| match event.event {
            EventKind::ServerStarted | EventKind::ServerStopping | EventKind::ServerStopped => {
                Some(event.event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        server_events,
        vec![
            EventKind::ServerStarted,
            EventKind::ServerStopping,
            EventKind::ServerStopped
        ]
    );
    assert!(events.iter().all(|event| event.timestamp_ms == 42_000));
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
}

#[test]
fn memberships_keep_owner_identity_reservations_and_join_order_separate_from_slots() {
    use eksplode_re_net::room::{MembershipStatus, RandomSource, RoomError, RoomMembers};
    use std::time::Duration;
    struct Random(common::FakeCredentials);
    impl RandomSource for Random {
        fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
            self.0
                .fill(output)
                .map_err(|_| RoomError::RandomUnavailable)
        }
    }
    let random = Random(common::FakeCredentials::default());
    let mut members = RoomMembers::default();
    let first = members.admit(3, 10, &random).unwrap().membership_id.clone();
    let second = members.admit(1, 11, &random).unwrap().membership_id.clone();
    assert_ne!(first, second);
    assert_eq!(members.owner_membership_id.as_deref(), Some(first.as_str()));
    assert!(members.admit(1, 12, &random).is_err());
    assert!(members.admit(4, 12, &random).is_err());
    assert!(members.reserve(10, Duration::from_secs(5)));
    let reserved = members.get(&first).unwrap();
    assert_eq!(reserved.status, MembershipStatus::Reserved);
    assert_eq!(reserved.connection_id, None);
    assert_eq!(reserved.reserved_until, Some(Duration::from_secs(65)));
    assert_eq!(members.owner_membership_id.as_deref(), Some(first.as_str()));
    assert!(members.expire(Duration::from_secs(64)).is_empty());
    assert_eq!(members.expire(Duration::from_secs(65)).len(), 1);
    assert_eq!(
        members.owner_membership_id.as_deref(),
        Some(second.as_str())
    );
    let proof = members.get(&second).unwrap().proof.to_secret();
    assert!(!format!("{members:?}").contains(proof.expose()));
}

#[test]
fn revision_exhaustion_is_a_typed_error_without_wrapping() {
    use eksplode_re_net::room::{RoomError, RoomRevision};
    let mut revision = RoomRevision::new(u64::MAX - 1);
    assert_eq!(revision.advance().unwrap(), u64::MAX);
    assert_eq!(revision.advance(), Err(RoomError::CounterExhausted));
    assert_eq!(revision.get(), u64::MAX);
}

#[test]
fn control_and_input_budgets_have_fixed_bounded_allowances() {
    use eksplode_re_net::room::{ControlBudget, InputBudget};
    use std::time::Duration;
    let mut control = ControlBudget::default();
    for _ in 0..40 {
        assert!(control.allow(Duration::ZERO));
    }
    assert!(!control.allow(Duration::ZERO));
    assert!(control.allow(Duration::from_millis(50)));
    assert!(!control.allow(Duration::from_millis(50)));
    let mut input = InputBudget::default();
    for _ in 0..60 {
        assert!(input.allow(Duration::ZERO));
    }
    assert!(!input.allow(Duration::from_millis(999)));
    assert!(input.allow(Duration::from_secs(1)));
}

#[test]
fn a_valid_object_is_read_as_it_is() {
    let s = from_wire(&json!({
        "name": "Friday room", "mode": "team", "size": "medium", "minutes": 5, "friendly_fire": false
    }));
    assert_eq!(s.name, "Friday room");
    assert_eq!(s.mode, Mode::Team);
    assert_eq!(s.size, BoardSize::Medium);
    assert_eq!(s.minutes, 5);
    assert!(!s.friendly_fire);
}

#[test]
fn anything_that_is_not_an_object_gives_the_defaults() {
    for v in [
        json!(null),
        json!(7),
        json!("team"),
        json!([1, 2]),
        json!(true),
    ] {
        assert_eq!(from_wire(&v), RoomSettings::default(), "{v}");
    }
}

#[test]
fn missing_wrong_and_unknown_fields_fall_back_per_field() {
    let s = from_wire(&json!({
        "name": 5, "mode": 3, "size": ["large"], "minutes": "5", "friendly_fire": "no",
        "extra": {"deep": [1, 2, 3]}
    }));
    assert_eq!(s, RoomSettings::default());

    let s = from_wire(&json!({"mode": "battle", "size": "huge", "minutes": 4}));
    assert_eq!(s.mode, Mode::Normal);
    assert_eq!(s.size, BoardSize::Small);
    assert_eq!(s.minutes, 3);

    let s = from_wire(&json!({"minutes": 999999999999u64}));
    assert_eq!(s.minutes, 3);
    let s = from_wire(&json!({"minutes": -2}));
    assert_eq!(s.minutes, 3);
    let s = from_wire(&json!({"minutes": 2.5}));
    assert_eq!(s.minutes, 3);

    let s = from_wire(&json!({"size": "large", "name": "Only these"}));
    assert_eq!(s.size, BoardSize::Large);
    assert_eq!(s.name, "Only these");
    assert_eq!(s.minutes, 3);
}

#[test]
fn the_name_is_cleaned_and_cut() {
    let s = from_wire(&json!({"name": "  \u{7}Hi\u{0}\nthere  "}));
    assert_eq!(s.name, "Hithere");
    let s = from_wire(&json!({"name": "x".repeat(60_000)}));
    assert_eq!(s.name.chars().count(), 24);
    let s = from_wire(&json!({"name": "\u{1}\u{2}\t "}));
    assert_eq!(s.name, "");
}

#[test]
fn the_wire_names_are_lowercase_strings() {
    let s = RoomSettings {
        name: "A".into(),
        mode: Mode::Chaser,
        size: BoardSize::Large,
        minutes: 10,
        friendly_fire: false,
    };
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(
        v,
        json!({"name":"A","mode":"chaser","size":"large","minutes":10,"friendly_fire":false})
    );
    assert_eq!(from_wire(&v), s);
    for (mode, name) in [
        (Mode::Normal, "normal"),
        (Mode::Team, "team"),
        (Mode::Chaser, "chaser"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), json!(name));
    }
    for (size, name) in [
        (BoardSize::Small, "small"),
        (BoardSize::Medium, "medium"),
        (BoardSize::Large, "large"),
    ] {
        assert_eq!(serde_json::to_value(size).unwrap(), json!(name));
    }
}
