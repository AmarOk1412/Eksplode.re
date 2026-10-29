#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{BoardSize, Lobby, Mode, RoomSettings};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::Host;
use serde_json::{json, Value};
use std::io::Write;
use std::time::{Duration, Instant};

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

fn v2_reply(raw: &mut Raw, kind: &str) -> Value {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let value = raw.recv_value().expect("v2 reply before timeout");
        if value["type"] == kind {
            return value;
        }
    }
    panic!("expected {kind}");
}

fn v2_create(host: &Host, name: &str, capacity: u8) -> (Raw, Value) {
    let mut raw = Raw::connect(host.addr());
    assert_eq!(raw.hello_v2().unwrap()["type"], "Hello");
    raw.send_value(&json!({"type":"CreateRoom","request_id":"create","name":name,
        "settings":{"name":"Same name","mode":"normal","size":"small","minutes":3,"friendly_fire":true},
        "capacity":capacity}));
    let welcome = v2_reply(&mut raw, "Welcome");
    (raw, welcome)
}

#[test]
fn v2_players_choose_independent_rooms_without_implicit_admission() {
    let host = host();
    let mut browser = Raw::connect(host.addr());
    browser.hello_v2().unwrap();
    browser.send_value(&json!({"type":"ListRooms","request_id":"empty","scope":"local"}));
    assert!(v2_reply(&mut browser, "Rooms")["rooms"]
        .as_array()
        .unwrap()
        .is_empty());
    let (mut owner, first) = v2_create(&host, "Ann", 4);
    let (_other_owner, second) = v2_create(&host, "Bob", 4);
    assert_ne!(first["room_id"], second["room_id"]);
    assert_eq!(first["owner_membership_id"], first["membership_id"]);
    browser.send_value(&json!({"type":"ListRooms","request_id":"list","scope":"local"}));
    let listed = v2_reply(&mut browser, "Rooms");
    let rooms = listed["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms
        .iter()
        .all(|room| room["occupied"] == 1 && room.get("resume_proof").is_none()));
    browser.join_room_v2("join", &first, "Cara");
    let joined = v2_reply(&mut browser, "Welcome");
    assert_eq!(joined["room_id"], first["room_id"]);
    assert_eq!(joined["slot"], 1);
    owner.send_value(&json!({"type":"Start","request_id":"wrong","room_id":second["room_id"]}));
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "NotInRoom");
}

fn v2_create_settings(host: &Host, name: &str, settings: Value) -> (Raw, Value) {
    let mut raw = Raw::connect(host.addr());
    assert_eq!(raw.hello_v2().unwrap()["type"], "Hello");
    raw.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":name,
        "settings":settings, "capacity":4}),
    );
    let welcome = v2_reply(&mut raw, "Welcome");
    v2_reply(&mut raw, "RoomUpdated");
    (raw, welcome)
}

#[test]
fn protected_room_admission_verifies_passwords_without_public_secrets() {
    let host = host();
    let password = "protected-room-password-canary";
    let mut owner = Raw::connect(host.addr());
    owner.hello_v2().unwrap();
    owner.send_value(
        &json!({"type":"CreateRoom", "request_id":"protected-create", "name":"Owner",
        "settings":RoomSettings::default(), "capacity":2, "password":password}),
    );
    let welcome = owner.recv_value().unwrap();
    assert_eq!(welcome["type"], "Welcome", "protected creation was refused");
    assert_eq!(welcome["password_required"], true);
    let initial_update = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(initial_update["password_required"], true);
    owner.send_value(
        &json!({"type":"CreateRoom", "request_id":"protected-create", "name":"Owner",
        "settings":RoomSettings::default(), "capacity":2, "password":password}),
    );
    let replay = v2_reply(&mut owner, "Welcome");
    assert_eq!(
        replay, welcome,
        "a lost reply replays the original private result"
    );
    owner.send_value(&json!({"type":"CancelRequest", "request_id":"after-commit", "target_request_id":"protected-create"}));
    let committed = v2_reply(&mut owner, "RequestStatus");
    assert_eq!(committed["status"], "committed");
    assert_eq!(committed["result"], welcome);
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.send_value(&json!({"type":"RequestStatus", "request_id":"private-query", "target_request_id":"protected-create"}));
    let private = v2_reply(&mut member, "RequestStatus");
    assert_eq!(private["status"], "unknown");
    assert!(private["result"].is_null());
    for (index, supplied) in [None, Some("wrong-password")].into_iter().enumerate() {
        member.send_value(&json!({"type":"JoinRoom", "request_id":format!("bad-password-{index}"),
            "name":"Member", "room_id":welcome["room_id"], "server_epoch":welcome["server_epoch"], "password":supplied}));
        let error = v2_reply(&mut member, "Error");
        assert_eq!(error["code"], "BadPassword");
        assert!(!error.to_string().contains(password));
        member.send_value(&json!({"type":"ListRooms", "request_id":format!("directory-{index}"), "scope":"local"}));
        let directory = v2_reply(&mut member, "Rooms");
        assert_eq!(directory["rooms"][0]["occupied"], 1);
        assert_eq!(directory["rooms"][0]["password_required"], true);
        assert!(!directory.to_string().contains(password));
        assert!(directory["rooms"][0].get("resume_proof").is_none());
    }
    member.send_value(
        &json!({"type":"JoinRoom", "request_id":"correct-password", "name":"Member",
        "room_id":welcome["room_id"], "server_epoch":welcome["server_epoch"], "password":password}),
    );
    let joined = v2_reply(&mut member, "Welcome");
    assert_eq!(joined["slot"], 1);
    assert_eq!(joined["room_id"], welcome["room_id"]);
    assert_eq!(joined["password_required"], true);
    let updated = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(updated["password_required"], true);
    assert!(!updated.to_string().contains(password));
    assert!(updated.get("resume_proof").is_none());
}

#[test]
fn five_wrong_passwords_from_one_source_are_denied_then_rate_limited() {
    let host = host();
    let password = "rate-limit-password-canary";
    let mut owner = Raw::connect(host.addr());
    owner.hello_v2().unwrap();
    owner.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Owner",
        "settings":RoomSettings::default(), "capacity":4, "password":password}),
    );
    let room = v2_reply(&mut owner, "Welcome");

    let mut attacker = Raw::connect(host.addr());
    attacker.hello_v2().unwrap();
    for attempt in 0..5 {
        attacker.send_value(
            &json!({"type":"JoinRoom", "request_id":format!("guess-{attempt}"),
            "name":"Attacker", "room_id":room["room_id"], "server_epoch":room["server_epoch"],
            "password":"wrong-password-guess"}),
        );
        assert_eq!(v2_reply(&mut attacker, "Error")["code"], "BadPassword");
    }
    attacker.send_value(&json!({"type":"JoinRoom", "request_id":"guess-6",
        "name":"Attacker", "room_id":room["room_id"], "server_epoch":room["server_epoch"],
        "password":"wrong-password-guess"}));
    assert_eq!(v2_reply(&mut attacker, "Error")["code"], "RateLimited");
    // the correct password is denied too: the source is rate-limited, not just bad guesses
    attacker.send_value(&json!({"type":"JoinRoom", "request_id":"guess-7",
        "name":"Attacker", "room_id":room["room_id"], "server_epoch":room["server_epoch"],
        "password":password}));
    assert_eq!(v2_reply(&mut attacker, "Error")["code"], "RateLimited");

    // a different source is unaffected
    let mut other = Raw::connect(host.addr());
    other.hello_v2().unwrap();
    other.send_value(&json!({"type":"JoinRoom", "request_id":"other-join",
        "name":"Other", "room_id":room["room_id"], "server_epoch":room["server_epoch"],
        "password":password}));
    assert_eq!(v2_reply(&mut other, "Welcome")["slot"], 1);
}

#[test]
fn protected_room_final_seat_is_committed_once_after_concurrent_verification() {
    let host = host();
    let mut owner = Raw::connect(host.addr());
    owner.hello_v2().unwrap();
    owner.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Owner",
        "settings":RoomSettings::default(), "capacity":2, "password":"race-password"}),
    );
    let room = v2_reply(&mut owner, "Welcome");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut attempts = Vec::new();
    for index in 0..2 {
        let address = host.addr();
        let room = room.clone();
        let barrier = barrier.clone();
        attempts.push(std::thread::spawn(move || {
            let mut peer = Raw::connect(address);
            peer.hello_v2().unwrap();
            barrier.wait();
            peer.send_value(&json!({"type":"JoinRoom", "request_id":"join", "name":format!("P{index}"),
                "room_id":room["room_id"], "server_epoch":room["server_epoch"], "password":"race-password"}));
            let result = peer.recv_value().unwrap();
            (peer, result)
        }));
    }
    let outcomes: Vec<_> = attempts
        .into_iter()
        .map(|attempt| attempt.join().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, reply)| reply["type"] == "Welcome")
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, reply)| reply["code"] == "Full")
            .count(),
        1
    );
    owner.send_value(&json!({"type":"ListRooms", "request_id":"after-race", "scope":"local"}));
    let directory = v2_reply(&mut owner, "Rooms");
    assert_eq!(directory["rooms"][0]["occupied"], 2);
    assert_eq!(directory["rooms"][0]["connected_humans"], 2);
}

#[test]
fn owner_password_updates_are_atomic_omission_preserves_and_empty_removes_protection() {
    let host = host();
    let (mut owner, mut room) = v2_create_settings(
        &host,
        "Owner",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    owner.send_value(
        &json!({"type":"SetRoom", "request_id":"protect", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":room["lobby"]["settings"], "capacity":4,
        "password":"owner-password-canary"}),
    );
    let reply = owner.recv_value().unwrap();
    assert_eq!(
        reply["type"], "RoomUpdated",
        "owner password change was refused"
    );
    assert_eq!(reply["password_required"], true);
    assert_eq!(
        reply["revision"].as_u64().unwrap(),
        room["revision"].as_u64().unwrap() + 1
    );
    assert!(!reply.to_string().contains("owner-password-canary"));
    room = reply;
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.join_room_v2("missing-password", &room, "Member");
    assert_eq!(v2_reply(&mut member, "Error")["code"], "BadPassword");
    let mut settings = room["lobby"]["settings"].clone();
    settings["name"] = json!("Still protected");
    owner.send_value(
        &json!({"type":"SetRoom", "request_id":"omit-password", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4}),
    );
    room = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(room["password_required"], true);
    member.send_value(&json!({"type":"JoinRoom", "request_id":"correct-password", "name":"Member",
        "room_id":room["room_id"], "server_epoch":room["server_epoch"], "password":"owner-password-canary"}));
    v2_reply(&mut member, "Welcome");
    room = v2_reply(&mut owner, "RoomUpdated");
    member.send_value(
        &json!({"type":"SetRoom", "request_id":"denied-password", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4, "password":""}),
    );
    assert_eq!(v2_reply(&mut member, "Error")["code"], "NotOwner");
    let mut invalid_settings = settings.clone();
    invalid_settings["name"] = json!("");
    owner.send_value(&json!({"type":"SetRoom", "request_id":"invalid-clear", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":invalid_settings, "capacity":4, "password":""}));
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "BadRoom");
    owner.send_value(&json!({"type":"SetRoom", "request_id":"replace-password", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4, "password":"replacement-password-canary"}));
    let replacement = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(
        replacement["revision"].as_u64().unwrap(),
        room["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(replacement["password_required"], true);
    assert!(!replacement
        .to_string()
        .contains("replacement-password-canary"));
    room = replacement;
    member.send_value(&json!({"type":"SetReady", "request_id":"still-member", "room_id":room["room_id"], "ready":true}));
    room = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(room["lobby"]["slots"][1]["ready"], true);
    let mut returning = Raw::connect(host.addr());
    returning.hello_v2().unwrap();
    returning.send_value(&json!({"type":"JoinRoom", "request_id":"old-password", "name":"Returning",
        "room_id":room["room_id"], "server_epoch":room["server_epoch"], "password":"owner-password-canary"}));
    assert_eq!(v2_reply(&mut returning, "Error")["code"], "BadPassword");
    returning.send_value(&json!({"type":"JoinRoom", "request_id":"new-password", "name":"Returning",
        "room_id":room["room_id"], "server_epoch":room["server_epoch"], "password":"replacement-password-canary"}));
    assert_eq!(v2_reply(&mut returning, "Welcome")["slot"], 2);
    room = v2_reply(&mut owner, "RoomUpdated");
    owner.send_value(
        &json!({"type":"SetRoom", "request_id":"clear-password", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4, "password":""}),
    );
    room = v2_reply(&mut owner, "RoomUpdated");
    assert_eq!(room["password_required"], false);
    let mut fresh = Raw::connect(host.addr());
    fresh.hello_v2().unwrap();
    fresh.join_room_v2("unprotected-join", &room, "Fresh");
    assert_eq!(v2_reply(&mut fresh, "Welcome")["slot"], 3);
}

#[test]
fn owner_removal_revokes_membership_frees_slot_and_allows_only_a_fresh_rejoin() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 2);
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.join_room_v2("member-join", &room, "Member");
    let admission = v2_reply(&mut member, "Welcome");
    let membership_id = admission["membership_id"].as_str().unwrap().to_string();
    assert_eq!(admission["slot"], 1);

    owner.send_value(&json!({"type":"RemovePlayer", "request_id":"remove-member",
        "room_id":room["room_id"], "membership_id":membership_id}));
    let after_removal = admission["revision"].as_u64().unwrap() + 1;
    let updated = loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["revision"].as_u64() == Some(after_removal) {
            break updated;
        }
    };
    assert_eq!(updated["lobby"]["slots"][1]["controller"], "Empty");
    assert!(updated["memberships"]
        .as_array()
        .unwrap()
        .iter()
        .all(|membership| membership["membership_id"] != membership_id));
    let removed = v2_reply(&mut member, "Removed");
    assert_eq!(removed["room_id"], room["room_id"]);
    assert_eq!(removed["reason"], "owner removal");

    member.send_value(
        &json!({"type":"Start", "request_id":"stale-start", "room_id":room["room_id"]}),
    );
    assert_eq!(v2_reply(&mut member, "Error")["code"], "NotInRoom");
    member.join_room_v2("fresh-join", &room, "Member");
    let fresh = v2_reply(&mut member, "Welcome");
    assert_eq!(fresh["slot"], 1);
    assert_ne!(fresh["membership_id"], membership_id);
    assert_eq!(fresh["owner_membership_id"], room["membership_id"]);
}

#[test]
fn room_name_creation_counts_trimmed_characters() {
    let host = host();
    for (index, expected) in ["x".to_string(), "x".repeat(24), "\u{e9}".repeat(24)]
        .into_iter()
        .enumerate()
    {
        let mut peer = Raw::connect(host.addr());
        peer.hello_v2().unwrap();
        let settings = RoomSettings {
            name: format!("\u{2003} {expected}  "),
            ..RoomSettings::default()
        };
        peer.send_value(
            &json!({"type":"CreateRoom", "request_id":format!("trimmed-{index}"),
            "name":"Owner", "settings":settings, "capacity":4}),
        );
        let reply = peer.recv_value().unwrap();
        assert_eq!(
            reply["type"], "Welcome",
            "a valid trimmed name was rejected"
        );
        assert_eq!(reply["lobby"]["settings"]["name"], expected);
    }
}

#[test]
fn room_name_updates_count_trimmed_characters_and_reject_controls_atomically() {
    let host = host();
    let (mut owner, mut room) = v2_create_settings(
        &host,
        "Owner",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    for (index, expected) in ["x".repeat(24), "\u{e9}".repeat(24)]
        .into_iter()
        .enumerate()
    {
        let mut settings = room["lobby"]["settings"].clone();
        settings["name"] = json!(format!("  {expected}\u{2003}"));
        owner.send_value(&json!({"type":"SetRoom", "request_id":format!("trimmed-{index}"),
            "room_id":room["room_id"], "expected_revision":room["revision"], "settings":settings, "capacity":4}));
        let reply = owner.recv_value().unwrap();
        assert_eq!(
            reply["type"], "RoomUpdated",
            "a valid trimmed name was rejected"
        );
        assert_eq!(reply["lobby"]["settings"]["name"], expected);
        room = reply;
    }
    for (index, name) in ["\nValid", "Valid\t", "x\u{7}y"].into_iter().enumerate() {
        let mut settings = room["lobby"]["settings"].clone();
        settings["name"] = json!(name);
        owner.send_value(&json!({"type":"SetRoom", "request_id":format!("control-{index}"),
            "room_id":room["room_id"], "expected_revision":room["revision"], "settings":settings, "capacity":4}));
        assert_eq!(v2_reply(&mut owner, "Error")["code"], "BadRoom");
    }
    owner.send_value(&json!({"type":"ListRooms", "request_id":"after-controls", "scope":"local"}));
    let directory = v2_reply(&mut owner, "Rooms");
    assert_eq!(directory["rooms"][0]["revision"], room["revision"]);
    assert_eq!(
        directory["rooms"][0]["name"],
        room["lobby"]["settings"]["name"]
    );
}

#[test]
fn room_name_updates_reject_empty_and_overlong_names_without_changes() {
    let host = host();
    let (mut owner, room) = v2_create_settings(
        &host,
        "Owner",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    for (index, name) in [
        String::new(),
        " \u{2003} ".into(),
        "x".repeat(25),
        "\u{e9}".repeat(25),
    ]
    .into_iter()
    .enumerate()
    {
        let mut settings = room["lobby"]["settings"].clone();
        settings["name"] = json!(name);
        owner.send_value(&json!({"type":"SetRoom", "request_id":format!("invalid-name-{index}"),
            "room_id":room["room_id"], "expected_revision":room["revision"], "settings":settings, "capacity":4}));
        let reply = owner.recv_value().unwrap();
        assert_eq!(reply["type"], "Error", "invalid name changed the room");
        assert_eq!(reply["code"], "BadRoom");
    }
    owner.send_value(
        &json!({"type":"ListRooms", "request_id":"after-invalid-names", "scope":"local"}),
    );
    let directory = v2_reply(&mut owner, "Rooms");
    assert_eq!(directory["rooms"][0]["revision"], room["revision"]);
    assert_eq!(
        directory["rooms"][0]["name"],
        room["lobby"]["settings"]["name"]
    );
}

#[test]
fn v2_only_one_of_a_hundred_connections_takes_the_last_seat() {
    let host = host();
    let (_owner, room) = v2_create(&host, "Owner", 2);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(100));
    let attempts: Vec<_> = (0..100)
        .map(|index| {
            let room = room.clone();
            let barrier = barrier.clone();
            let address = host.addr();
            std::thread::spawn(move || {
                let mut raw = Raw::connect(address);
                raw.hello_v2().unwrap();
                barrier.wait();
                raw.join_room_v2("join", &room, &format!("p{index}"));
                let reply = raw.recv_value().unwrap();
                (raw, reply)
            })
        })
        .collect();
    let outcomes: Vec<_> = attempts
        .into_iter()
        .map(|attempt| attempt.join().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, reply)| reply["type"] == "Welcome")
            .count(),
        1
    );
    assert!(outcomes
        .iter()
        .all(|(_, reply)| reply["type"] == "Welcome" || reply["code"] == "Full"));
}

#[test]
fn expired_room_creation_never_publishes_a_room_or_membership() {
    use eksplode_re_net::host::HostServices;
    use eksplode_re_net::room::{RandomSource, RoomError, ServiceClock};
    use eksplode_re_net::HostConfig;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Clock(common::FakeClock);
    impl ServiceClock for Clock {
        fn now(&self) -> Duration {
            self.0.now()
        }
    }
    struct Random {
        clock: common::FakeClock,
        calls: AtomicUsize,
        bytes: common::FakeCredentials,
    }
    impl RandomSource for Random {
        fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 3 {
                self.clock.advance(6_000);
            }
            self.bytes
                .fill(output)
                .map_err(|_| RoomError::RandomUnavailable)
        }
    }
    let clock = common::FakeClock::default();
    let host = Host::start_with_services(
        HostConfig {
            bind: "127.0.0.1:0".into(),
            ..Default::default()
        },
        HostServices::new(
            Arc::new(Clock(clock.clone())),
            Arc::new(Random {
                clock,
                calls: AtomicUsize::new(0),
                bytes: common::FakeCredentials::default(),
            }),
        ),
    )
    .unwrap();
    let mut raw = Raw::connect(host.addr());
    raw.hello_v2().unwrap();
    raw.create_room_v2("create", "Ann");
    assert_eq!(v2_reply(&mut raw, "Error")["code"], "RequestExpired");
    raw.send_value(&json!({"type":"ListRooms","request_id":"after","scope":"local"}));
    assert!(v2_reply(&mut raw, "Rooms")["rooms"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn v2_directory_pages_are_bounded_and_mutation_invalidates_the_cursor() {
    let host = host_with(|config| config.limits.max_rooms = 40);
    let _owners: Vec<_> = (0..33)
        .map(|index| v2_create(&host, &format!("p{index}"), 4).0)
        .collect();
    let mut browser = Raw::connect(host.addr());
    browser.hello_v2().unwrap();
    browser.send_value(&json!({"type":"ListRooms","request_id":"first","scope":"local"}));
    let first = v2_reply(&mut browser, "Rooms");
    assert_eq!(first["rooms"].as_array().unwrap().len(), 32);
    let cursor = first["next_cursor"].as_str().unwrap();
    browser.send_value(
        &json!({"type":"ListRooms","request_id":"second","scope":"local","cursor":cursor}),
    );
    let second = v2_reply(&mut browser, "Rooms");
    assert_eq!(second["rooms"].as_array().unwrap().len(), 1);
    assert!(second.get("next_cursor").is_none());
    browser.send_value(&json!({"type":"ListRooms","request_id":"refresh","scope":"local"}));
    let before_change = v2_reply(&mut browser, "Rooms");
    let (_extra, _) = v2_create(&host, "extra", 4);
    browser.send_value(&json!({"type":"ListRooms","request_id":"stale","scope":"local","cursor":before_change["next_cursor"]}));
    assert_eq!(v2_reply(&mut browser, "Error")["code"], "StaleRevision");
}

#[test]
fn v2_running_rooms_use_independent_modes_and_reject_fresh_match_admission() {
    let host = host_with(|config| {
        config.game.block_density = 0.0;
        config.tick = Duration::from_millis(20);
    });
    let mut players = Vec::new();
    for mode in ["normal", "team", "chaser"] {
        let mut raw = Raw::connect(host.addr());
        raw.hello_v2().unwrap();
        raw.send_value(&json!({"type":"CreateRoom","request_id":"create","name":mode,
            "settings":{"name":mode,"mode":mode,"size":"small","minutes":3,"friendly_fire":true},"capacity":4}));
        let welcome = v2_reply(&mut raw, "Welcome");
        for index in 0..3 {
            raw.send_value(&json!({"type":"AddBot","request_id":format!("bot-{index}"),"room_id":welcome["room_id"]}));
        }
        raw.send_value(&json!({"type":"Start","request_id":"start","room_id":welcome["room_id"]}));
        let start = v2_reply(&mut raw, "Start");
        assert_eq!(start["room_id"], welcome["room_id"]);
        assert_eq!(start["config"]["mode"], mode);
        let snapshot = v2_reply(&mut raw, "Snapshot");
        assert_eq!(snapshot["room_id"], welcome["room_id"]);
        players.push((raw, welcome));
    }
    let mut late = Raw::connect(host.addr());
    late.hello_v2().unwrap();
    late.join_room_v2("late", &players[0].1, "late");
    assert_eq!(v2_reply(&mut late, "Error")["code"], "InMatch");
    for (raw, welcome) in &mut players {
        for _ in 0..3 {
            let snapshot = v2_reply(raw, "Snapshot");
            assert_eq!(snapshot["room_id"], welcome["room_id"]);
        }
    }
}

#[test]
fn v2_explicit_leave_closes_only_its_room_and_reuses_the_unjoined_transport() {
    let host = host();
    let (mut owner, first) = v2_create(&host, "First", 4);
    let (_other, second) = v2_create(&host, "Second", 4);
    owner.join_room_v2("already", &second, "First");
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "AlreadyInRoom");
    owner.send_value(&json!({"type":"LeaveRoom","request_id":"leave","room_id":first["room_id"]}));
    assert_eq!(
        v2_reply(&mut owner, "LeftRoom")["room_id"],
        first["room_id"]
    );
    owner.send_value(&json!({"type":"ListRooms","request_id":"after","scope":"local"}));
    let directory = v2_reply(&mut owner, "Rooms");
    assert_eq!(directory["rooms"].as_array().unwrap().len(), 1);
    assert_eq!(directory["rooms"][0]["room_id"], second["room_id"]);
    owner.join_room_v2("new-join", &second, "First");
    assert_eq!(
        v2_reply(&mut owner, "Welcome")["room_id"],
        second["room_id"]
    );
}

#[test]
fn v2_startup_bots_fill_each_created_room_without_displacing_the_owner() {
    let host = host_with(|config| config.default_bots = 3);
    let (_first, first) = v2_create(&host, "First", 2);
    let (_second, second) = v2_create(&host, "Second", 4);
    assert_eq!(first["slot"], 0);
    assert_eq!(second["slot"], 0);
    let count = |welcome: &Value| {
        welcome["lobby"]["slots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|slot| slot["controller"] == "Bot")
            .count()
    };
    assert_eq!(count(&first), 1);
    assert_eq!(count(&second), 3);
}

#[test]
fn v2_developer_state_is_private_to_the_selected_room() {
    let host = host_with(|config| config.dev = true);
    let (mut first, first_room) = v2_create(&host, "First", 4);
    let (mut second, second_room) = v2_create(&host, "Second", 4);
    let first_state = v2_reply(&mut first, "DevState");
    let second_state = v2_reply(&mut second, "DevState");
    assert_eq!(first_state["room_id"], first_room["room_id"]);
    assert_eq!(second_state["room_id"], second_room["room_id"]);
    first.send_value(&json!({"type":"DevPauseBots","request_id":"pause","room_id":first_room["room_id"],"paused":true}));
    assert_eq!(v2_reply(&mut first, "DevState")["paused"], true);
    second.send_value(&json!({"type":"DevPauseBots","request_id":"check","room_id":second_room["room_id"],"paused":false}));
    let unchanged = v2_reply(&mut second, "DevState");
    assert_eq!(unchanged["room_id"], second_room["room_id"]);
    assert_eq!(unchanged["paused"], false);
}

#[test]
fn v2_invalid_admissions_and_room_limits_do_not_mutate_existing_rooms() {
    let host = host_with(|config| config.limits.max_rooms = 1);
    let (_owner, room) = v2_create(&host, "Owner", 4);
    let mut outsider = Raw::connect(host.addr());
    outsider.hello_v2().unwrap();
    outsider.create_room_v2("full-server", "Other");
    assert_eq!(v2_reply(&mut outsider, "Error")["code"], "ServerFull");
    outsider.join_room_v2("bad-name", &room, "a name that is too long");
    assert_eq!(v2_reply(&mut outsider, "Error")["code"], "BadName");
    let mut wrong_epoch = room.clone();
    wrong_epoch["server_epoch"] = json!("old-epoch");
    outsider.join_room_v2("wrong-epoch", &wrong_epoch, "Other");
    assert_eq!(v2_reply(&mut outsider, "Error")["code"], "RoomNotFound");
    outsider.send_value(&json!({"type":"ListRooms","request_id":"after","scope":"local"}));
    let directory = v2_reply(&mut outsider, "Rooms");
    assert_eq!(directory["rooms"].as_array().unwrap().len(), 1);
    assert_eq!(directory["rooms"][0]["occupied"], 1);
}

#[test]
fn a_negotiated_room_connection_cannot_join_the_legacy_lobby_too() {
    let host = host();
    let (mut owner, _) = v2_create(&host, "Owner", 4);
    owner.send_value(&json!({"type":"Join","version":1,"name":"Duplicate"}));
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline);
        let reply = owner.recv_value().unwrap();
        if reply["type"] == "RoomUpdated" {
            continue;
        }
        assert_eq!(reply["type"], "Error");
        assert_eq!(reply["code"], "BadVersion");
        break;
    }
}

#[test]
fn invalid_creation_settings_never_publish_an_unplayable_room() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.hello_v2().unwrap();
    for (index, (minutes, capacity, mode)) in [
        (0, 4, "normal"),
        (4, 4, "normal"),
        (255, 4, "normal"),
        (3, 0, "normal"),
        (3, 5, "normal"),
        (3, 3, "team"),
    ]
    .into_iter()
    .enumerate()
    {
        raw.send_value(&json!({"type":"CreateRoom","request_id":format!("invalid-{index}"),"name":"Owner",
            "settings":{"name":"Invalid","mode":mode,"size":"small","minutes":minutes,"friendly_fire":true},"capacity":capacity}));
        let error = v2_reply(&mut raw, "Error");
        assert!(error["code"] == "BadRoom" || error["code"] == "ModeNeedsPlayers");
    }
    raw.send_value(&json!({"type":"ListRooms","request_id":"after","scope":"local"}));
    assert!(v2_reply(&mut raw, "Rooms")["rooms"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn three_four_participant_modes_complete_twenty_isolated_rounds_each() {
    let host = host_with(|config| {
        config.game.round_limit_ticks = 12;
        config.game.block_density = 0.0;
        config.default_bots = 3;
    });
    let mut rooms = Vec::new();
    for mode in ["normal", "team", "chaser"] {
        let mut raw = Raw::connect(host.addr());
        raw.hello_v2().unwrap();
        raw.send_value(&json!({"type":"CreateRoom","request_id":"create","name":mode,
            "settings":{"name":mode,"mode":mode,"size":"small","minutes":3,"friendly_fire":true},"capacity":4}));
        let welcome = v2_reply(&mut raw, "Welcome");
        assert_eq!(
            welcome["lobby"]["slots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|slot| slot["controller"] != "Empty")
                .count(),
            4
        );
        rooms.push((raw, welcome));
    }
    let workers: Vec<_> = rooms.into_iter().map(|(mut raw, welcome)| std::thread::spawn(move || {
        for round in 0..20 {
            raw.send_value(&json!({"type":"Start","request_id":format!("round-{round}"),"room_id":welcome["room_id"]}));
            let deadline = Instant::now() + WAIT;
            loop {
                assert!(Instant::now() < deadline, "round {round} did not finish");
                let message = raw.recv_value().expect("room remains connected");
                if message.get("room_id").is_some() { assert_eq!(message["room_id"], welcome["room_id"]); }
                assert_ne!(message["type"], "Error", "{message}");
                if message["type"] == "Ended" { break; }
            }
        }
        raw
    })).collect();
    let _connected: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
}

#[test]
fn v2_existing_owner_settings_are_atomic_scoped_and_kept_for_the_match() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 4);
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.join_room_v2("join", &room, "Member");
    let joined = v2_reply(&mut member, "Welcome");
    let settings =
        json!({"name":"Changed","mode":"normal","size":"medium","minutes":5,"friendly_fire":false});
    member.send_value(
        &json!({"type":"SetRoom","request_id":"denied","room_id":room["room_id"],
        "expected_revision":joined["revision"],"settings":settings,"capacity":4}),
    );
    let denied = v2_reply(&mut member, "Error");
    assert_eq!(denied["code"], "NotOwner");
    assert!(denied["message"].as_str().unwrap().contains("Only Owner"));
    owner.send_value(
        &json!({"type":"SetRoom","request_id":"change","room_id":room["room_id"],
        "expected_revision":joined["revision"],"settings":settings,"capacity":4}),
    );
    loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["lobby"]["settings"]["name"] == "Changed" {
            break;
        }
    }
    owner.send_value(&json!({"type":"Start","request_id":"start","room_id":room["room_id"]}));
    assert_eq!(v2_reply(&mut owner, "Start")["config"]["width"], 19);
    owner.send_value(
        &json!({"type":"SetRoom","request_id":"during","room_id":room["room_id"],
        "expected_revision":3,"settings":settings,"capacity":4}),
    );
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "InMatch");
}

#[test]
fn in_process_host_controls_require_the_selected_room_reference() {
    let host = host();
    let (mut owner, welcome) = v2_create(&host, "Owner", 4);
    let reference = eksplode_re_net::protocol::v2::RoomRef {
        server_id: welcome["server_id"].as_str().unwrap().into(),
        server_epoch: welcome["server_epoch"].as_str().unwrap().into(),
        room_id: welcome["room_id"].as_str().unwrap().into(),
    };
    host.handle().add_room_bot(&reference).unwrap();
    loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["lobby"]["slots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slot| slot["controller"] == "Bot")
        {
            break;
        }
    }
    host.handle().start_room(&reference).unwrap();
    assert_eq!(v2_reply(&mut owner, "Start")["room_id"], welcome["room_id"]);
}

fn lobby_settings(raw: &mut Raw, pred: impl Fn(&RoomSettings) -> bool) -> RoomSettings {
    while let Some(message) = raw.recv() {
        if let ServerMsg::Lobby { lobby, .. } = message.into_game_message() {
            if pred(&lobby.settings) {
                return lobby.settings;
            }
        }
    }
    panic!("no matching lobby");
}

#[test]
fn f4_us1_as1_directory_is_unjoined_and_creation_is_explicit() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    let hello = raw.hello_v2().unwrap();
    assert_eq!(
        hello["defaults"],
        serde_json::to_value(RoomSettings::default()).unwrap()
    );
    for request in ["directory-1", "directory-2"] {
        raw.send_value(&json!({"type":"ListRooms", "request_id":request, "scope":"local"}));
        assert!(v2_reply(&mut raw, "Rooms")["rooms"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    let mut settings = hello["defaults"].clone();
    settings["name"] = json!("Friday");
    settings["size"] = json!("medium");
    settings["minutes"] = json!(5);
    raw.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Ann",
        "settings":settings, "capacity":4}),
    );
    let welcome = v2_reply(&mut raw, "Welcome");
    assert_eq!(welcome["slot"], 0, "directory queries took no seat");
    assert_eq!(welcome["owner_slot"], 0);
    assert_eq!(welcome["lobby"]["settings"], settings);
    raw.send_value(&json!({"type":"ListRooms", "request_id":"after", "scope":"local"}));
    let directory = v2_reply(&mut raw, "Rooms");
    assert_eq!(directory["rooms"][0]["room_id"], welcome["room_id"]);
    assert_eq!(directory["rooms"][0]["connected_humans"], 1);
}

#[test]
fn creation_with_server_defaults_gets_a_name_from_the_owner() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    let hello = raw.hello_v2().unwrap();
    raw.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Ann",
        "settings":hello["defaults"], "capacity":4}),
    );
    let lobby: Lobby =
        serde_json::from_value(v2_reply(&mut raw, "Welcome")["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.name, "Ann's room");
    assert_eq!(lobby.settings.mode, Mode::Normal);
    assert_eq!(lobby.settings.size, BoardSize::Small);
    assert_eq!(lobby.settings.minutes, 3);
}

#[test]
fn the_server_defaults_are_what_a_closed_room_offers() {
    let host = host_with(|c| {
        c.defaults = RoomSettings {
            name: "Lab".into(),
            size: BoardSize::Large,
            minutes: 10,
            ..RoomSettings::default()
        }
    });
    let mut raw = Raw::connect(host.addr());
    let hello = raw.hello_v2().unwrap();
    let settings: RoomSettings = serde_json::from_value(hello["defaults"].clone()).unwrap();
    assert_eq!(settings.size, BoardSize::Large);
    raw.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Ann",
        "settings":settings, "capacity":4}),
    );
    let lobby: Lobby =
        serde_json::from_value(v2_reply(&mut raw, "Welcome")["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.name, "Lab");
    assert_eq!(lobby.settings.minutes, 10);
}

#[test]
fn f4_us1_as2_a_second_player_joins_the_open_room_and_cannot_change_it() {
    let host = host();
    let settings = RoomSettings {
        name: "Friday".into(),
        minutes: 5,
        ..RoomSettings::default()
    };
    let (_a, room) = v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    let other = json!({"name": "Mine", "size": "large", "minutes": 10});
    let mut b = Raw::connect(host.addr());
    b.hello_v2().unwrap();
    b.send_value(
        &json!({"type":"JoinRoom", "request_id":"join", "room_id":room["room_id"],
        "server_epoch":room["server_epoch"], "name":"Bob", "settings":other}),
    );
    let welcome = v2_reply(&mut b, "Welcome");
    assert_eq!(welcome["slot"], 1);
    let lobby: Lobby = serde_json::from_value(welcome["lobby"].clone()).unwrap();
    assert_eq!(
        lobby.settings.name, "Friday",
        "joining cannot replace the selected room's settings"
    );
    assert_eq!(lobby.settings.minutes, 5);
    assert_eq!(lobby.settings.size, BoardSize::Small);

    b.send_value(
        &json!({"type":"SetRoom", "request_id":"denied", "room_id":room["room_id"],
        "expected_revision":welcome["revision"], "settings":{"minutes":2}, "capacity":4}),
    );
    assert!(b.recv_until(is_error("NotOwner")).is_some());
    b.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    let directory = v2_reply(&mut b, "Rooms");
    assert_eq!(directory["rooms"][0]["minutes"], 5);
    assert_eq!(directory["rooms"][0]["connected_humans"], 2);
}

#[test]
fn f4_us1_as3_the_owner_changes_the_settings_and_everybody_sees_them() {
    let host = host_with(|c| c.game = quick_game(30));
    let (mut a, room) = v2_create_settings(
        &host,
        "Ann",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    let mut b = Raw::connect(host.addr());
    b.hello_v2().unwrap();
    b.join_room_v2("join", &room, "Bob");
    let joined = v2_reply(&mut b, "Welcome");
    let mut settings = room["lobby"]["settings"].clone();
    settings["name"] = json!("New name");
    settings["size"] = json!("large");
    settings["minutes"] = json!(10);
    settings["mode"] = json!("battle");
    a.send_value(
        &json!({"type":"SetRoom", "request_id":"invalid", "room_id":room["room_id"],
        "expected_revision":joined["revision"], "settings":settings, "capacity":4}),
    );
    assert_eq!(v2_reply(&mut a, "Error")["code"], "BadRoom");
    settings["mode"] = json!("normal");
    a.send_value(
        &json!({"type":"SetRoom", "request_id":"change", "room_id":room["room_id"],
        "expected_revision":joined["revision"], "settings":settings, "capacity":4}),
    );
    let want = |s: &RoomSettings| s.name == "New name";
    for raw in [&mut a, &mut b] {
        let s = lobby_settings(raw, want);
        assert_eq!(s.size, BoardSize::Large);
        assert_eq!(s.minutes, 10);
        assert_eq!(
            s.mode,
            Mode::Normal,
            "a valid update follows atomic rejection of an unknown mode"
        );
    }
    // the next match uses them
    a.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
    match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
        Some(ServerMsg::Start { config, .. }) => {
            assert_eq!((config.width, config.height), (25, 19));
            assert_eq!(config.round_limit_ticks, 12_000);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_set_room_that_is_not_an_object_is_refused_with_bad_room() {
    let host = host();
    let (mut a, room) = v2_create_settings(
        &host,
        "Ann",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    for (index, settings) in [json!(5), json!("team")].into_iter().enumerate() {
        a.send_value(&json!({"type":"SetRoom", "request_id":format!("bad-{index}"), "room_id":room["room_id"],
            "expected_revision":room["revision"], "settings":settings, "capacity":4}));
        assert_eq!(v2_reply(&mut a, "Error")["code"], "BadRoom");
    }
    a.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    let directory = v2_reply(&mut a, "Rooms");
    assert_eq!(directory["rooms"][0]["name"], "Ann's room");
    assert_eq!(directory["rooms"][0]["revision"], room["revision"]);
}

#[test]
fn f4_us1_as4_settings_are_refused_during_a_match_and_kept_for_the_rematch() {
    let host = host_with(|c| c.game = quick_game(400));
    let settings = RoomSettings {
        name: "Keep me".into(),
        size: BoardSize::Medium,
        ..RoomSettings::default()
    };
    let (mut a, room) = v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    a.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
    assert!(a
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
    let mut settings = room["lobby"]["settings"].clone();
    settings["name"] = json!("Too late");
    a.send_value(
        &json!({"type":"SetRoom", "request_id":"too-late", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4}),
    );
    assert!(a.recv_until(is_error("InMatch")).is_some());

    // the practice round of one participant ends by time; the lobby comes back
    assert!(a
        .recv_until(|m| matches!(m, ServerMsg::Ended { .. }))
        .is_some());
    let s = lobby_settings(&mut a, |_| true);
    assert_eq!(s.name, "Keep me");
    assert_eq!(s.size, BoardSize::Medium);
}

#[test]
fn f4_us1_as8_the_room_closes_when_the_last_person_leaves() {
    let host = host();
    let settings = RoomSettings {
        name: "Gone soon".into(),
        size: BoardSize::Large,
        ..RoomSettings::default()
    };
    let (mut peer, room) =
        v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    peer.send_value(&json!({"type":"LeaveRoom", "request_id":"leave", "room_id":room["room_id"]}));
    assert_eq!(v2_reply(&mut peer, "LeftRoom")["room_id"], room["room_id"]);
    peer.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    assert!(v2_reply(&mut peer, "Rooms")["rooms"]
        .as_array()
        .unwrap()
        .is_empty());
    peer.send_value(
        &json!({"type":"CreateRoom", "request_id":"create-again", "name":"Bob",
        "settings":RoomSettings::default(), "capacity":4}),
    );
    let welcome = v2_reply(&mut peer, "Welcome");
    assert_ne!(welcome["room_id"], room["room_id"]);
    assert_eq!(welcome["slot"], 0);
    let lobby: Lobby = serde_json::from_value(welcome["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.size, BoardSize::Small);
    assert_eq!(lobby.settings.name, "Bob's room");
}

#[test]
fn a_legacy_join_is_rejected_without_implicit_admission() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send_line(r#"{"type":"Join","version":1,"name":"Old"}"#);
    assert_eq!(
        raw.recv_value().unwrap(),
        json!({"type":"Rejected", "reason":"BadVersion"})
    );
    assert!(raw.recv_value().is_none());
    let directory = host.handle().directory(None).unwrap();
    match directory {
        eksplode_re_net::protocol::v2::Reply::Rooms { rooms, .. } => assert!(rooms.is_empty()),
        _ => panic!("expected directory"),
    }
}

#[test]
fn a_hostile_creation_is_rejected_and_the_unjoined_transport_remains_usable() {
    let host = host();
    let hostile = json!({
        "name": "A".repeat(50_000), "mode": {"x": 1}, "size": [[]], "minutes": 1e30,
        "friendly_fire": null, "junk": {"a": {"b": {"c": [1, 2, 3]}}}
    });
    let mut peer = Raw::connect(host.addr());
    peer.hello_v2().unwrap();
    peer.send_value(
        &json!({"type":"CreateRoom", "request_id":"hostile", "name":"Ann",
        "settings":hostile, "capacity":4}),
    );
    assert_eq!(v2_reply(&mut peer, "Error")["code"], "BadRoom");
    peer.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    assert!(v2_reply(&mut peer, "Rooms")["rooms"]
        .as_array()
        .unwrap()
        .is_empty());
    peer.send_value(
        &json!({"type":"CreateRoom", "request_id":"valid", "name":"Ann",
        "settings":RoomSettings::default(), "capacity":4}),
    );
    let welcome = v2_reply(&mut peer, "Welcome");
    assert_eq!(welcome["slot"], 0);
    let lobby: Lobby = serde_json::from_value(welcome["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.name, "Ann's room");
    assert_eq!(lobby.settings.minutes, 3);
    assert_eq!(lobby.settings.size, BoardSize::Small);
}

#[test]
fn f4_us1_as6_the_match_uses_the_room_size_and_time() {
    let sizes = [("small", 15u8, 13u8), ("medium", 19, 15), ("large", 25, 19)];
    for (size, w, h) in sizes {
        for minutes in [2u32, 3, 5, 10] {
            let host = host();
            let settings = json!({"name":"Matrix", "mode":"normal", "size":size,
                "minutes":minutes, "friendly_fire":true});
            let (mut a, room) = v2_create_settings(&host, "Ann", settings);
            a.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
            let config = match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
                Some(ServerMsg::Start { config, .. }) => config,
                other => panic!("{other:?}"),
            };
            assert_eq!((config.width, config.height), (w, h), "{size}");
            assert_eq!(
                config.round_limit_ticks,
                minutes * 60 * 20,
                "{size} {minutes}"
            );
            // the pattern always starts 60 seconds before the end
            assert_eq!(
                config.round_limit_ticks - config.closing_ticks,
                (minutes - 1) * 60 * 20
            );
            // the first snapshot carries the same configuration and the arena of that size
            match a.recv_until(|m| matches!(m, ServerMsg::Snapshot { .. })) {
                Some(ServerMsg::Snapshot { state, .. }) => {
                    assert_eq!((state.arena.width(), state.arena.height()), (w, h));
                    assert_eq!(state.config.round_limit_ticks, config.round_limit_ticks);
                }
                other => panic!("{other:?}"),
            }
        }
    }
}

#[test]
fn a_change_by_the_owner_before_the_start_is_used() {
    let host = host();
    let (mut a, mut room) = v2_create_settings(
        &host,
        "Ann",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    for (index, size, minutes) in [(0, "medium", 5), (1, "large", 2)] {
        let mut settings = room["lobby"]["settings"].clone();
        settings["size"] = json!(size);
        settings["minutes"] = json!(minutes);
        a.send_value(&json!({"type":"SetRoom", "request_id":format!("change-{index}"), "room_id":room["room_id"],
            "expected_revision":room["revision"], "settings":settings, "capacity":4}));
        room = v2_reply(&mut a, "RoomUpdated");
        assert_eq!(room["lobby"]["settings"]["minutes"], minutes);
    }
    a.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
    match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
        Some(ServerMsg::Start { config, .. }) => {
            assert_eq!(
                (config.width, config.height, config.round_limit_ticks),
                (25, 19, 2400)
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_team_room_is_accepted_and_the_other_modes_follow_their_rules() {
    let host = host();
    let settings = RoomSettings {
        mode: Mode::Team,
        ..RoomSettings::default()
    };
    let (mut a, room) = v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    let lobby: Lobby = serde_json::from_value(room["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.mode, Mode::Team);
    let settings = RoomSettings {
        friendly_fire: false,
        ..lobby.settings
    };
    a.send_value(
        &json!({"type":"SetRoom", "request_id":"change", "room_id":room["room_id"],
        "expected_revision":room["revision"], "settings":settings, "capacity":4}),
    );
    let s: RoomSettings =
        serde_json::from_value(v2_reply(&mut a, "RoomUpdated")["lobby"]["settings"].clone())
            .unwrap();
    assert_eq!(s.mode, Mode::Team);
    assert!(!s.friendly_fire);
}

#[test]
fn f4_us1_as5_a_start_with_too_few_participants_is_refused_with_the_missing_count() {
    let host = host();
    let a = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let room = a
        .create_room(
            "Ann",
            &RoomSettings {
                mode: Mode::Team,
                friendly_fire: false,
                ..RoomSettings::default()
            },
            4,
        )
        .unwrap();
    welcome_slot(&a);
    for (missing, add_bot) in [(3, true), (2, true), (1, false)] {
        a.send(ClientMsg::Start).unwrap();
        let message = match wait_for_game(&a, is_error("ModeNeedsPlayers")) {
            Some(ServerMsg::Error { message, .. }) => message,
            other => panic!("{other:?}"),
        };
        assert!(message.contains(&format!("{missing} missing")), "{message}");
        assert!(
            message.contains("bots"),
            "the message says bots can fill the slots: {message}"
        );
        if add_bot {
            a.send(ClientMsg::AddBot).unwrap();
            wait_for_game(&a, |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() > 4 - missing as usize))
                .unwrap();
        }
    }
    // a second human and two bots make four
    let b = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    b.join_room(&room, "Bob").unwrap();
    a.send(ClientMsg::AddBot).unwrap();
    a.send(ClientMsg::AddBot).unwrap();
    wait_for_game(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4),
    )
    .unwrap();
    a.send(ClientMsg::Start).unwrap();
    match wait_for_game(&a, |m| matches!(m, ServerMsg::Start { .. })) {
        Some(ServerMsg::Start { config, .. }) => {
            assert_eq!(config.mode, Mode::Team);
            assert!(!config.friendly_fire);
        }
        other => panic!("{other:?}"),
    }
    // teams reach the observation
    match wait_for_game(&a, |m| matches!(m, ServerMsg::Snapshot { .. })) {
        Some(ServerMsg::Snapshot { state, .. }) => {
            let teams: Vec<_> = state.players.iter().map(|p| p.team).collect();
            assert_eq!(teams, vec![Some(0), Some(0), Some(1), Some(1)]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f4_us4_dev_commands_still_work_in_a_chaser_room() {
    let host = host_with(|c| c.dev = true);
    let settings = RoomSettings {
        mode: Mode::Chaser,
        ..RoomSettings::default()
    };
    let (mut a, room) = v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    let lobby: Lobby = serde_json::from_value(room["lobby"].clone()).unwrap();
    assert_eq!(lobby.settings.mode, Mode::Chaser);
    a.send_value(&json!({"type":"DevPauseBots", "request_id":"pause", "room_id":room["room_id"], "paused":true}));
    a.send_value(
        &json!({"type":"DevSetDrops", "request_id":"drops", "room_id":room["room_id"],
        "chances":[0.5,0,0,0,0,0,0,0]}),
    );
    let state = a.recv_until(|m| matches!(m, ServerMsg::DevState { drops, .. } if drops[0] == 0.5));
    match state {
        Some(ServerMsg::DevState { paused, drops }) => {
            assert!(paused);
            assert_eq!(drops[0], 0.5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_chaser_room_starts_with_four_participants_and_one_chaser() {
    let host = host();
    let a = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    a.create_room(
        "Ann",
        &RoomSettings {
            mode: Mode::Chaser,
            ..RoomSettings::default()
        },
        4,
    )
    .unwrap();
    welcome_slot(&a);
    a.send(ClientMsg::Start).unwrap();
    match wait_for_game(&a, is_error("ModeNeedsPlayers")) {
        Some(ServerMsg::Error { message, .. }) => assert!(
            message.contains("Chaser") && message.contains("3 missing"),
            "{message}"
        ),
        other => panic!("{other:?}"),
    }
    for _ in 0..3 {
        a.send(ClientMsg::AddBot).unwrap();
    }
    wait_for_game(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4),
    )
    .unwrap();
    a.send(ClientMsg::Start).unwrap();
    match wait_for_game(&a, |m| matches!(m, ServerMsg::Snapshot { .. })) {
        Some(ServerMsg::Snapshot { state, .. }) => {
            use eksplode_re_core::Role;
            assert_eq!(
                state
                    .players
                    .iter()
                    .filter(|p| p.role == Role::Chaser)
                    .count(),
                1
            );
            assert_eq!(
                state
                    .players
                    .iter()
                    .filter(|p| p.role == Role::Runner)
                    .count(),
                3
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f4_random_and_truncated_room_frames_never_break_the_room() {
    let host = host();
    let settings = RoomSettings {
        name: "Steady".into(),
        size: BoardSize::Medium,
        ..RoomSettings::default()
    };
    let (mut owner, before) =
        v2_create_settings(&host, "Ann", serde_json::to_value(settings).unwrap());
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..4 {
        let mut bad = Raw::connect(host.addr());
        bad.hello_v2().unwrap();
        for _ in 0..30 {
            let text = match next() % 7 {
                0 => json!({"type":"ListRooms", "request_id":"list", "scope":"local"}).to_string(),
                1 => json!({"type":"SetRoom", "request_id":"bad-settings", "room_id":before["room_id"],
                    "expected_revision":before["revision"], "capacity":4,
                    "settings":{"mode":7,"size":[],"minutes":"x"}}).to_string(),
                2 => r#"{"type":"SetRoom"}"#.to_string(),
                3 => r#"{"type":"SetRoom","settings":{"name":"#.to_string(),
                4 => json!({"type":"JoinRoom", "request_id":"fuzz-join", "room_id":before["room_id"],
                    "server_epoch":before["server_epoch"], "name":format!("M{round}"),
                    "settings":{"minutes":next()}}).to_string(),
                5 => String::from_utf8_lossy(&(0..40).map(|_| next() as u8).collect::<Vec<_>>())
                    .to_string(),
                _ => r#"{"type":"SetRoom","settings":{"name":"Hijack"}}"#.to_string(),
            };
            if bad.stream.write_all(text.as_bytes()).is_err()
                || bad.stream.write_all(b"\n").is_err()
            {
                break; // too many bad messages: the host closed the connection
            }
        }
    }
    // the owner and the room are untouched and the room still works
    owner.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    let directory = v2_reply(&mut owner, "Rooms");
    assert_eq!(directory["rooms"].as_array().unwrap().len(), 1);
    assert_eq!(directory["rooms"][0]["room_id"], before["room_id"]);
    assert_eq!(
        directory["rooms"][0]["name"],
        before["lobby"]["settings"]["name"]
    );
    assert_eq!(directory["rooms"][0]["size"], "medium");
}

#[test]
fn f4_a_set_room_line_over_the_limit_closes_the_connection_and_changes_nothing() {
    let host = host();
    let (mut a, before) = v2_create_settings(
        &host,
        "Ann",
        serde_json::to_value(RoomSettings::default()).unwrap(),
    );
    let mut observer = Raw::connect(host.addr());
    observer.hello_v2().unwrap();
    observer.join_room_v2("observe", &before, "Observer");
    v2_reply(&mut observer, "Welcome");
    let big = format!(
        r#"{{"type":"SetRoom","settings":{{"name":"{}"}}}}"#,
        "x".repeat(70 * 1024)
    );
    a.send_line(&big);
    // an Error says why, then the connection is closed (recv gives None)
    let mut saw_oversize_name = false;
    while let Some(m) = a.recv() {
        if let ServerMsg::Lobby { lobby, .. } = m {
            saw_oversize_name |= lobby.settings.name.len() > 100;
        }
    }
    assert!(!saw_oversize_name);
    observer.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    let directory = v2_reply(&mut observer, "Rooms");
    assert_eq!(directory["rooms"][0]["room_id"], before["room_id"]);
    assert_eq!(
        directory["rooms"][0]["name"],
        before["lobby"]["settings"]["name"]
    );
}

#[test]
fn set_room_capacity_cannot_drop_below_occupied_slots() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 4);
    let mut first = Raw::connect(host.addr());
    first.hello_v2().unwrap();
    first.join_room_v2("first-join", &room, "First");
    v2_reply(&mut first, "Welcome");
    let mut second = Raw::connect(host.addr());
    second.hello_v2().unwrap();
    second.join_room_v2("second-join", &room, "Second");
    let joined = v2_reply(&mut second, "Welcome");

    owner.send_value(&json!({"type":"SetRoom", "request_id":"shrink-too-far",
        "room_id":room["room_id"], "expected_revision":joined["revision"],
        "settings":room["lobby"]["settings"], "capacity":2}));
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "CapacityConflict");

    owner.send_value(&json!({"type":"SetRoom", "request_id":"shrink-to-fit",
        "room_id":room["room_id"], "expected_revision":joined["revision"],
        "settings":room["lobby"]["settings"], "capacity":3}));
    let updated = loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["capacity"] == 3 {
            break updated;
        }
    };
    assert_eq!(updated["capacity"], 3);
}

#[test]
fn v2_start_addbot_and_removebot_require_the_room_owner() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 4);
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.join_room_v2("member-join", &room, "Member");
    v2_reply(&mut member, "Welcome");
    v2_reply(&mut owner, "RoomUpdated");

    member.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
    assert_eq!(v2_reply(&mut member, "Error")["code"], "NotOwner");
    member.send_value(&json!({"type":"AddBot", "request_id":"add-bot", "room_id":room["room_id"]}));
    assert_eq!(v2_reply(&mut member, "Error")["code"], "NotOwner");
    member.send_value(
        &json!({"type":"RemoveBot", "request_id":"remove-bot", "room_id":room["room_id"]}),
    );
    assert_eq!(v2_reply(&mut member, "Error")["code"], "NotOwner");

    owner.send_value(
        &json!({"type":"AddBot", "request_id":"owner-add-bot", "room_id":room["room_id"]}),
    );
    let updated = loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["lobby"]["slots"][2]["controller"] == "Bot" {
            break updated;
        }
    };
    assert_eq!(updated["lobby"]["slots"][2]["controller"], "Bot");
}

#[test]
fn removing_a_player_requires_ownership_and_a_real_target() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 4);
    let mut first = Raw::connect(host.addr());
    first.hello_v2().unwrap();
    first.join_room_v2("first-join", &room, "First");
    let first_admission = v2_reply(&mut first, "Welcome");
    let first_id = first_admission["membership_id"].as_str().unwrap();
    let mut second = Raw::connect(host.addr());
    second.hello_v2().unwrap();
    second.join_room_v2("second-join", &room, "Second");
    v2_reply(&mut second, "Welcome");

    first.send_value(
        &json!({"type":"RemovePlayer", "request_id":"non-owner-remove",
        "room_id":room["room_id"], "membership_id":first_id}),
    );
    assert_eq!(v2_reply(&mut first, "Error")["code"], "NotOwner");

    owner.send_value(
        &json!({"type":"RemovePlayer", "request_id":"unknown-target",
        "room_id":room["room_id"], "membership_id":"no-such-membership"}),
    );
    assert_eq!(v2_reply(&mut owner, "Error")["code"], "NotInRoom");
}

#[test]
fn owner_removal_during_a_match_replaces_the_player_with_a_bot_and_play_continues() {
    let host = host();
    let (mut owner, room) = v2_create(&host, "Owner", 2);
    let mut member = Raw::connect(host.addr());
    member.hello_v2().unwrap();
    member.join_room_v2("member-join", &room, "Member");
    let admission = v2_reply(&mut member, "Welcome");
    let membership_id = admission["membership_id"].as_str().unwrap().to_string();
    v2_reply(&mut owner, "RoomUpdated");

    owner.send_value(&json!({"type":"Start", "request_id":"start", "room_id":room["room_id"]}));
    v2_reply(&mut owner, "Start");

    owner.send_value(&json!({"type":"RemovePlayer", "request_id":"remove-member",
        "room_id":room["room_id"], "membership_id":membership_id}));
    let removed = v2_reply(&mut member, "Removed");
    assert_eq!(removed["room_id"], room["room_id"]);
    let updated = loop {
        let updated = v2_reply(&mut owner, "RoomUpdated");
        if updated["lobby"]["slots"][1]["controller"] == "Bot" {
            break updated;
        }
    };
    assert!(updated["memberships"]
        .as_array()
        .unwrap()
        .iter()
        .all(|membership| membership["membership_id"] != membership_id));
}
