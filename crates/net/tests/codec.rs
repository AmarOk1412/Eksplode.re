#![allow(clippy::unwrap_used)]

mod common;

use std::io::Cursor;

use eksplode_re_core::{Config, Controller, Dir, Event, GameState, Lobby, Outcome, PlayerId, Pos};
use eksplode_re_net::codec::{decode, encode, LineReader, NetError};
use eksplode_re_net::protocol::{valid_name, ClientMsg, RejectReason, ServerMsg, MAX_LINE};

fn roundtrip<T>(msg: T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let bytes = encode(&msg).unwrap();
    assert_eq!(*bytes.last().unwrap(), b'\n');
    assert_eq!(
        bytes.iter().filter(|b| **b == b'\n').count(),
        1,
        "one line per message"
    );
    let back: T = decode(&bytes[..bytes.len() - 1]).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn room_fixtures_share_monotonic_time_credentials_and_captured_events() {
    let clock = common::FakeClock::default();
    let shared_clock = clock.clone();
    assert_eq!(clock.now(), std::time::Duration::ZERO);
    clock.advance(60_000);
    assert_eq!(shared_clock.now(), std::time::Duration::from_secs(60));

    let credentials = common::FakeCredentials::default();
    let mut first = [0; 32];
    let mut second = [0; 32];
    credentials.fill(&mut first).unwrap();
    credentials.clone().fill(&mut second).unwrap();
    assert_ne!(first, second);
    credentials.fail();
    assert!(credentials.fill(&mut second).is_err());

    let events = common::CapturedEvents::default();
    events
        .clone()
        .push(serde_json::json!({"event": "room.created"}));
    assert_eq!(events.snapshot()[0]["event"], "room.created");
}

#[test]
fn v2_control_requests_round_trip_and_keep_secrets_out_of_debug() {
    use eksplode_re_net::protocol::v2::Request;
    use serde_json::json;

    let settings = serde_json::to_value(eksplode_re_core::RoomSettings::default()).unwrap();
    let requests = vec![
        json!({"type":"Hello","version":2}),
        json!({"type":"ListRooms","request_id":"list","scope":"known"}),
        json!({"type":"CreateRoom","request_id":"create","name":"Ann","settings":settings,
            "capacity":4,"password":"password-canary"}),
        json!({"type":"JoinRoom","request_id":"join","server_epoch":"epoch","room_id":"room",
            "name":"Ann","password":"password-canary"}),
        json!({"type":"ResumeRoom","request_id":"resume","server_epoch":"epoch","room_id":"room",
            "membership_id":"member","proof":"proof-canary"}),
        json!({"type":"AckResume","request_id":"ack","resume_request_id":"resume",
            "room_id":"room","proof":"proof-canary"}),
        json!({"type":"RequestStatus","request_id":"query","target_request_id":"create"}),
        json!({"type":"CancelRequest","request_id":"cancel","target_request_id":"create"}),
        json!({"type":"SetRoom","request_id":"settings","room_id":"room","expected_revision":1,
            "settings":settings,"capacity":4,"password":""}),
        json!({"type":"RemovePlayer","request_id":"remove","room_id":"room","membership_id":"member"}),
        json!({"type":"LeaveRoom","request_id":"leave","room_id":"room"}),
        json!({"type":"Start","request_id":"start","room_id":"room"}),
        json!({"type":"AddBot","request_id":"add","room_id":"room"}),
        json!({"type":"RemoveBot","request_id":"bot","room_id":"room"}),
        json!({"type":"SetReady","request_id":"ready","room_id":"room","ready":true}),
        json!({"type":"SetLook","request_id":"look","room_id":"room","look":{}}),
        json!({"type":"Input","room_id":"room","tick":7,"mv":"Left","bomb":true,"punch":false}),
        json!({"type":"DevPauseBots","request_id":"pause","room_id":"room","paused":true}),
        json!({"type":"DevSetDrops","request_id":"drops","room_id":"room","chances":[0.0,1.0]}),
        json!({"type":"DevResetDrops","request_id":"reset","room_id":"room"}),
        json!({"type":"DevSetBotBehaviour","request_id":"behaviour","room_id":"room",
            "behaviour":"balanced"}),
        json!({"type":"Ping","nonce":42}),
    ];
    for value in requests {
        let request: Request = serde_json::from_value(value).unwrap();
        let debug = format!("{request:?}");
        assert!(!debug.contains("password-canary"));
        assert!(!debug.contains("proof-canary"));
        roundtrip(request);
    }
}

#[test]
fn v2_request_ids_are_bounded_printable_ascii() {
    use eksplode_re_net::protocol::v2::Request;
    for request_id in [
        "".to_string(),
        "x".repeat(65),
        "bad\nname".into(),
        "nonascii-é".into(),
    ] {
        let value = serde_json::json!({"type":"ListRooms","request_id":request_id,"scope":"local"});
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
    for request_id in ["x".to_string(), "x".repeat(64)] {
        let value = serde_json::json!({"type":"ListRooms","request_id":request_id,"scope":"local"});
        assert!(serde_json::from_value::<Request>(value).is_ok());
    }
    assert!(decode::<Request>(br#"{"type":"Unknown","request_id":"test"}"#).is_err());
    assert!(decode::<Request>(&vec![b'x'; MAX_LINE + 1]).is_err());
}

#[test]
fn v2_hello_and_private_results_round_trip() {
    use eksplode_re_net::protocol::v2::Reply;
    use serde_json::json;
    let mut replies = vec![
        json!({"type":"Hello","version":2,"server_id":"test","server_epoch":"epoch",
            "capabilities":["rooms"],"limits":{"max_rooms":32,"max_connections":256}}),
        json!({"type":"Rooms","request_id":"list","sources":[],"rooms":[]}),
        json!({"type":"Error","request_id":"join","code":"BadPassword","message":"Admission denied"}),
        json!({"type":"RequestStatus","request_id":"status","target_request_id":"join","status":"pending"}),
        json!({"type":"Pong","nonce":5}),
    ];
    let reference = json!({"server_id":"test","server_epoch":"epoch","room_id":"room"});
    let lobby = Lobby::new();
    let admission = json!({
        "server_id":"test","server_epoch":"epoch","room_id":"room",
        "membership_id":"member","slot":0,"resume_proof":"private-result-canary",
        "reconnect_window_seconds":60,"owner_membership_id":"member","owner_slot":0,
        "revision":1,"capacity":4,"password_required":true,
        "config":Config::default(),"lobby":lobby,"looks":[],"dev":false
    });
    let mut welcome = admission.clone();
    welcome["type"] = json!("Welcome");
    welcome["request_id"] = json!("create");
    replies.push(welcome.clone());
    let mut resumed = admission;
    resumed["type"] = json!("Resumed");
    resumed["request_id"] = json!("resume");
    resumed["phase"] = json!("lobby");
    resumed["tick"] = json!(null);
    resumed["state"] = json!(null);
    resumed["result"] = json!(null);
    replies.push(resumed);
    replies.push(json!({"type":"RequestStatus","request_id":"status",
        "target_request_id":"create","status":"committed","result":welcome}));
    for (kind, fields) in [
        (
            "ResumeAcknowledged",
            json!({"request_id":"ack","resume_request_id":"resume"}),
        ),
        (
            "RoomUpdated",
            json!({"revision":2,"owner_membership_id":"member","owner_slot":0,
            "capacity":4,"password_required":true,"memberships":[],"lobby":lobby,"looks":[]}),
        ),
        ("LeftRoom", json!({"request_id":"leave"})),
        ("Removed", json!({"reason":"owner removal"})),
        ("RoomClosed", json!({"reason":"empty"})),
        ("Start", json!({"seed":7,"config":Config::default()})),
        (
            "Snapshot",
            json!({"tick":0,"state":GameState::new(Config::default(), 7, [true;4]).unwrap()
            .observe(PlayerId::new(0).unwrap()),"events":[]}),
        ),
        ("PlayerLeft", json!({"slot":1,"replaced_by_bot":true})),
        ("Ended", json!({"outcome":Outcome::Draw})),
        ("DevState", json!({"paused":true,"drops":vec![0.0; 8]})),
    ] {
        let mut value = reference.clone();
        value["type"] = json!(kind);
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        replies.push(value);
    }
    for value in replies {
        let reply = serde_json::from_value::<Reply>(value).unwrap();
        assert!(!format!("{reply:?}").contains("private-result-canary"));
        roundtrip(reply);
    }
}

#[test]
fn v2_room_commands_are_not_decoded_as_legacy_commands() {
    use eksplode_re_net::codec::decode_client;
    for text in [
        r#"{"type":"Hello","version":2}"#,
        r#"{"type":"Start","request_id":"start","room_id":"room"}"#,
        r#"{"type":"Input","room_id":"room","tick":7,"mv":null,"bomb":false}"#,
    ] {
        assert!(matches!(
            decode_client(text.as_bytes()).unwrap(),
            ClientMsg::RoomV2(_)
        ));
    }
    assert!(matches!(
        decode_client(br#"{"type":"Join","version":1,"name":"Ann"}"#).unwrap(),
        ClientMsg::Join { .. }
    ));
}

#[test]
fn v2_host_negotiates_without_joining_and_rejects_bad_versions() {
    let host = common::host();
    let mut raw = common::Raw::connect(host.addr());
    let hello = raw.hello_v2().unwrap();
    assert_eq!(hello["type"], "Hello");
    assert_eq!(hello["version"], 2);
    assert_eq!(hello["server_epoch"].as_str().unwrap().len(), 32);
    assert!(hello.get("membership_id").is_none());

    let mut old = common::Raw::connect(host.addr());
    old.send_value(&serde_json::json!({"type":"Hello","version":1}));
    let reply = old.recv_value().unwrap();
    assert_eq!(reply["type"], "Rejected");
    assert_eq!(reply["reason"], "BadVersion");
}

#[test]
fn v2_tcp_client_connects_without_joining_and_preserves_reply_request_ids() {
    use eksplode_re_net::protocol::v2::{Reply, Request, RequestId, Scope};
    let host = common::host();
    let client = eksplode_re_net::Client::connect(&host.addr().to_string()).unwrap();
    assert!(matches!(
        client.recv_timeout(common::WAIT).unwrap(),
        ServerMsg::RoomV2(reply) if matches!(*reply, Reply::Hello { version: 2, .. })
    ));
    client
        .send_v2(Request::ListRooms {
            request_id: RequestId::new("directory").unwrap(),
            scope: Scope::Local,
            cursor: None,
        })
        .unwrap();
    match client.recv_timeout(common::WAIT).unwrap() {
        ServerMsg::RoomV2(reply) => match *reply {
            Reply::Rooms {
                request_id, rooms, ..
            } => {
                assert_eq!(request_id.as_str(), "directory");
                assert!(rooms.is_empty());
            }
            other => panic!("expected directory, got {other:?}"),
        },
        other => panic!("v2 reply lost its identity: {other:?}"),
    }
}

#[test]
fn a_connection_that_never_negotiates_is_closed_after_two_seconds() {
    let host = common::host();
    let mut raw = common::Raw::connect(host.addr());
    let started = std::time::Instant::now();
    assert!(raw.recv_value().is_none());
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    assert!(started.elapsed() < common::WAIT);
}

#[test]
fn v2_request_status_is_connection_scoped_and_replays_terminal_outcomes() {
    let host = common::host();
    let mut first = common::Raw::connect(host.addr());
    first.hello_v2().unwrap();
    first.create_room_v2("create", "Ann");
    let original = first.recv_value().unwrap();
    assert_eq!(original["request_id"], "create");
    first.create_room_v2("create", "Ann");
    let deadline = std::time::Instant::now() + common::WAIT;
    let replay = loop {
        assert!(std::time::Instant::now() < deadline);
        let value = first.recv_value().unwrap();
        if value["type"] != "RoomUpdated" {
            break value;
        }
    };
    assert_eq!(replay, original);
    first.send_value(&serde_json::json!({"type":"RequestStatus", "request_id":"status", "target_request_id":"create"}));
    let status = first.recv_value().unwrap();
    assert_eq!(status["status"], "committed");
    assert_eq!(status["result"], original);
    let mut second = common::Raw::connect(host.addr());
    second.hello_v2().unwrap();
    second.send_value(&serde_json::json!({"type":"RequestStatus", "request_id":"status", "target_request_id":"create"}));
    assert_eq!(second.recv_value().unwrap()["status"], "unknown");
}

#[test]
fn client_gameplay_messages_use_only_an_explicitly_admitted_v2_room() {
    use eksplode_re_net::protocol::v2::{Reply, Request, RequestId};
    let host = common::host();
    let client = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    client.recv_timeout(common::WAIT).unwrap();
    client
        .send_v2(Request::create_room(
            RequestId::new("create").unwrap(),
            "Ann".into(),
            &eksplode_re_core::RoomSettings::default(),
            4,
        ))
        .unwrap();
    loop {
        if matches!(client.recv_timeout(common::WAIT).unwrap(), ServerMsg::RoomV2(reply)
            if matches!(reply.as_ref(), Reply::Welcome { .. }))
        {
            break;
        }
    }
    client.send(ClientMsg::AddBot).unwrap();
    let mut added = false;
    while let Ok(message) = client.recv_timeout(common::WAIT) {
        if let ServerMsg::RoomV2(reply) = message {
            if let Reply::RoomUpdated { lobby, .. } = *reply {
                if lobby
                    .slots
                    .iter()
                    .any(|slot| slot.controller == Controller::Bot)
                {
                    added = true;
                    break;
                }
            }
        }
    }
    assert!(
        added,
        "gameplay command did not retain the explicit room binding"
    );
}

#[test]
fn explicit_sdk_admission_retains_welcome_for_the_gameplay_consumer() {
    let host = common::host();
    let creator = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let reference = creator
        .create_room("Creator", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert!(common::wait_for(
        &creator,
        |message| matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), eksplode_re_net::protocol::v2::Reply::Welcome { .. }))
    )
    .is_some());
    let member = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    assert_eq!(member.join_room(&reference, "Member").unwrap(), reference);
    assert!(common::wait_for(
        &member,
        |message| matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), eksplode_re_net::protocol::v2::Reply::Welcome { .. }))
    )
    .is_some());
}

#[test]
fn unjoined_client_reports_upgrade_required_without_legacy_join_fallback() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(common::WAIT)).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["type"],
            "Hello"
        );
        stream
            .write_all(
                b"{\"type\":\"Error\",\"code\":\"Malformed\",\"message\":\"unknown Hello\"}\n",
            )
            .unwrap();
        line.clear();
        assert_eq!(
            reader.read_line(&mut line).unwrap(),
            0,
            "client must not send a legacy Join"
        );
    });
    let client = eksplode_re_net::Client::connect_unjoined(&address.to_string()).unwrap();
    assert_eq!(
        client.recv_timeout(common::WAIT).unwrap(),
        ServerMsg::Rejected {
            reason: RejectReason::BadVersion
        }
    );
    server.join().unwrap();
}

#[test]
fn unjoined_client_reports_upgrade_required_when_hello_times_out() {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(common::WAIT)).unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["type"],
            "Hello"
        );
        line.clear();
        assert_eq!(reader.read_line(&mut line).unwrap(), 0);
    });
    let client = eksplode_re_net::Client::connect_unjoined(&address.to_string()).unwrap();
    assert_eq!(
        client.recv_timeout(common::WAIT).unwrap(),
        ServerMsg::Rejected {
            reason: RejectReason::BadVersion
        }
    );
    server.join().unwrap();
}

#[test]
fn v2_control_flood_is_limited_without_disturbing_another_connection() {
    use eksplode_re_net::protocol::v2::{ErrorCode, Reply, Request};
    let host = common::host();
    let noisy = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    noisy.recv_timeout(common::WAIT).unwrap();
    for nonce in 0..80 {
        if noisy.send_v2(Request::Ping { nonce }).is_err() {
            break;
        }
    }
    let mut limited = false;
    while let Ok(message) = noisy.recv_timeout(std::time::Duration::from_millis(300)) {
        if matches!(message, ServerMsg::RoomV2(reply) if matches!(*reply, Reply::Error { code: ErrorCode::RateLimited, .. }))
        {
            limited = true;
            break;
        }
    }
    assert!(limited, "control flood was not rate limited");
    let other = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    other.recv_timeout(common::WAIT).unwrap();
    other.send_v2(Request::Ping { nonce: 999 }).unwrap();
    assert!(matches!(other.recv_timeout(common::WAIT).unwrap(),
        ServerMsg::RoomV2(reply) if matches!(*reply, Reply::Pong { nonce: 999 })));
}

#[test]
fn a_slow_in_process_reader_is_dropped_without_blocking_match_ticks() {
    let host = common::host();
    let slow = host.connect(true).unwrap();
    slow.send_v2(eksplode_re_net::protocol::v2::Request::Hello { version: 2 })
        .unwrap();
    let room = slow
        .create_room("slow", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    let observer = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    observer.join_room(&room, "observer").unwrap();
    common::welcome_slot(&observer);
    host.handle().start_room(&room).unwrap();
    assert!(common::wait_for_game(&observer, |message| matches!(
        message,
        ServerMsg::PlayerLeft {
            slot: 0,
            replaced_by_bot: true
        }
    ))
    .is_some());
    assert!(common::wait_for_game(&observer, |message| matches!(message,
        ServerMsg::Snapshot { tick, .. } if *tick >= 100))
    .is_some());
    let mut disconnected = false;
    for _ in 0..140 {
        match slow.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                disconnected = true;
                break;
            }
            Ok(_) => {}
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
        }
    }
    assert!(
        disconnected,
        "slow reader retained an unbounded reply backlog"
    );
}

#[test]
fn every_client_message_round_trips() {
    roundtrip(ClientMsg::Join {
        version: 1,
        name: "Ann".into(),
        look: None,
        room: None,
    });
    roundtrip(ClientMsg::SetReady { ready: true });
    roundtrip(ClientMsg::Input {
        tick: 7,
        mv: Some(Dir::Left),
        bomb: true,
        punch: false,
    });
    roundtrip(ClientMsg::Input {
        tick: 0,
        mv: None,
        bomb: false,
        punch: false,
    });
    roundtrip(ClientMsg::Start);
    roundtrip(ClientMsg::AddBot);
    roundtrip(ClientMsg::RemoveBot);
    roundtrip(ClientMsg::Leave);
    roundtrip(ClientMsg::Ping { nonce: 99 });
}

#[test]
fn every_server_message_round_trips() {
    let mut lobby = Lobby::new();
    lobby.assign(0, Controller::Remote, "Ann").unwrap();
    let state = GameState::new(Config::default(), 3, [true; 4]).unwrap();
    let me = PlayerId::new(2).unwrap();
    roundtrip(ServerMsg::Welcome {
        looks: vec![],
        slot: 1,
        config: Config::default(),
        lobby: lobby.clone(),
        dev: true,
    });
    roundtrip(ServerMsg::DevState {
        paused: true,
        drops: [0.0, 0.25, 0.5, 1.0, 0.0, 0.1, 0.2, 0.3],
        behaviour: Some("protective".into()),
    });
    roundtrip(ServerMsg::Rejected {
        reason: RejectReason::Full,
    });
    roundtrip(ServerMsg::Lobby {
        lobby,
        looks: vec![],
    });
    roundtrip(ServerMsg::Start {
        seed: 5,
        config: Config::default(),
    });
    roundtrip(ServerMsg::Snapshot {
        tick: 4,
        state: Box::new(state.observe(me)),
        events: vec![Event::Moved {
            player: me,
            from: Pos::new(1, 1),
            to: Pos::new(2, 1),
        }],
    });
    roundtrip(ServerMsg::PlayerLeft {
        slot: 2,
        replaced_by_bot: true,
    });
    roundtrip(ServerMsg::Ended {
        outcome: Outcome::Winner(me),
    });
    roundtrip(ServerMsg::Error {
        code: "X".into(),
        message: "y".into(),
    });
    roundtrip(ServerMsg::Pong { nonce: 1 });
}

#[test]
fn the_wire_format_is_tagged_json() {
    let bytes = encode(&ClientMsg::Ping { nonce: 5 }).unwrap();
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        "{\"type\":\"Ping\",\"nonce\":5}\n"
    );
}

#[test]
fn bad_input_gives_typed_errors_not_panics() {
    let bad: [&[u8]; 6] = [
        b"",
        b"not json",
        b"{\"type\":\"Nope\"}",
        b"{\"type\":\"Join\",\"version\":\"one\",\"name\":\"a\"}",
        b"{\"type\":\"Input\",\"tick\":-1,\"mv\":null,\"bomb\":false}",
        &[0xff, 0xfe, 0xfd],
    ];
    for line in bad {
        assert!(
            matches!(decode::<ClientMsg>(line), Err(NetError::Malformed(_))),
            "{line:?}"
        );
    }
}

#[test]
fn oversized_lines_are_rejected() {
    let big = vec![b'a'; MAX_LINE + 1];
    assert!(matches!(
        decode::<ClientMsg>(&big),
        Err(NetError::LineTooLong)
    ));

    let mut data = big.clone();
    data.push(b'\n');
    let mut r = LineReader::new(Cursor::new(data));
    assert!(matches!(r.read_line(), Err(NetError::LineTooLong)));

    // no newline at all must not buffer forever
    let mut r = LineReader::new(Cursor::new(vec![b'a'; MAX_LINE * 3]));
    assert!(matches!(r.read_line(), Err(NetError::LineTooLong)));
}

#[test]
fn a_line_of_exactly_the_limit_is_fine() {
    let mut data = vec![b'a'; MAX_LINE];
    data.push(b'\n');
    let mut r = LineReader::new(Cursor::new(data));
    assert_eq!(r.read_line().unwrap().unwrap().len(), MAX_LINE);
}

#[test]
fn the_reader_splits_lines_and_signals_the_end() {
    let mut r = LineReader::new(Cursor::new(b"one\ntwo\n\nthree".to_vec()));
    assert_eq!(r.read_line().unwrap().unwrap(), b"one");
    assert_eq!(r.read_line().unwrap().unwrap(), b"two");
    assert_eq!(r.read_line().unwrap().unwrap(), b"");
    assert!(
        matches!(r.read_line(), Err(NetError::Closed)),
        "unterminated last line"
    );
    assert!(r.read_line().unwrap().is_none());
}

#[test]
fn names_are_one_to_sixteen_printable_characters() {
    assert!(valid_name("Ann"));
    assert!(valid_name("a"));
    assert!(valid_name(&"x".repeat(16)));
    assert!(valid_name("Zoë ☃"));
    assert!(!valid_name(""));
    assert!(!valid_name("   "));
    assert!(!valid_name(&"x".repeat(17)));
    assert!(!valid_name("bad\nname"));
    assert!(!valid_name("tab\tname"));
}

#[test]
fn owner_commands_use_plain_tagged_json() {
    for (msg, text) in [
        (ClientMsg::Start, "{\"type\":\"Start\"}\n"),
        (ClientMsg::AddBot, "{\"type\":\"AddBot\"}\n"),
        (ClientMsg::RemoveBot, "{\"type\":\"RemoveBot\"}\n"),
    ] {
        assert_eq!(std::str::from_utf8(&encode(&msg).unwrap()).unwrap(), text);
    }
}

#[test]
fn refusal_codes_round_trip_as_errors() {
    for code in ["NotOwner", "CannotStart", "WrongPhase", "NoSlot"] {
        roundtrip(ServerMsg::error(code, "why"));
    }
}

#[test]
fn an_input_without_punch_means_no_punch() {
    let old = br#"{"type":"Input","tick":3,"mv":"Left","bomb":true}"#;
    match decode::<ClientMsg>(old).unwrap() {
        ClientMsg::Input {
            tick,
            mv,
            bomb,
            punch,
        } => {
            assert_eq!((tick, mv, bomb, punch), (3, Some(Dir::Left), true, false));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_input_with_punch_round_trips() {
    roundtrip(ClientMsg::Input {
        tick: 9,
        mv: None,
        bomb: false,
        punch: true,
    });
}

#[test]
fn the_observation_carries_punch_malus_and_flight() {
    use eksplode_re_core::{ActiveMalus, Flight, PowerUpKind};
    let mut s = GameState::new(Config::default(), 3, [true; 4]).unwrap();
    s.players[1].has_punch = true;
    s.players[1].malus = Some(ActiveMalus {
        kind: PowerUpKind::Slow,
        ticks_left: 120,
    });
    let bomb = eksplode_re_core::Bomb {
        owner: PlayerId::new(1).unwrap(),
        pos: Pos::new(3, 1),
        ticks_left: 30,
        range: 2,
        flight: Some(Flight {
            dir: Dir::Right,
            hop_left: 1,
            origin: Pos::new(2, 1),
        }),
        slide: None,
    };
    s.bombs.push(bomb);
    s.players[2].has_kick = true;
    s.bombs.push(eksplode_re_core::Bomb {
        owner: PlayerId::new(2).unwrap(),
        pos: Pos::new(5, 5),
        ticks_left: 30,
        range: 2,
        flight: None,
        slide: Some(eksplode_re_core::Slide {
            dir: Dir::Left,
            step_left: 2,
        }),
    });
    let msg = ServerMsg::Snapshot {
        tick: 1,
        state: Box::new(s.observe(PlayerId::new(0).unwrap())),
        events: vec![
            Event::BombPunched {
                player: PlayerId::new(1).unwrap(),
                from: Pos::new(2, 1),
                dir: Dir::Right,
            },
            Event::BombHopped {
                pos: Pos::new(3, 1),
            },
            Event::BombKicked {
                player: PlayerId::new(2).unwrap(),
                from: Pos::new(6, 5),
                dir: Dir::Left,
            },
            Event::BombSlid {
                pos: Pos::new(5, 5),
            },
            Event::BombStopped {
                pos: Pos::new(4, 5),
            },
            Event::MalusStarted {
                player: PlayerId::new(1).unwrap(),
                kind: PowerUpKind::Slow,
            },
        ],
    };
    let text = String::from_utf8(encode(&msg).unwrap()).unwrap();
    assert!(text.contains(r#""has_punch":true"#), "{text}");
    assert!(text.contains(r#""has_kick":true"#), "{text}");
    assert!(
        text.contains(r#""slide":{"dir":"Left","step_left":2}"#),
        "{text}"
    );
    assert!(
        text.contains(r#""malus":{"kind":"Slow","ticks_left":120}"#),
        "{text}"
    );
    assert!(
        text.contains(r#""flight":{"dir":"Right","hop_left":1"#),
        "{text}"
    );
    roundtrip(msg);
}

#[test]
fn old_config_json_without_the_new_values_still_decodes() {
    let mut value = serde_json::to_value(Config::default()).unwrap();
    let map = value.as_object_mut().unwrap();
    map.remove("malus_ticks");
    map.remove("punch_hop_ticks");
    let config: Config = serde_json::from_value(value).unwrap();
    assert_eq!((config.malus_ticks, config.punch_hop_ticks), (200, 2));
}

#[test]
fn every_dev_message_round_trips_and_old_welcomes_mean_no_dev() {
    roundtrip(ClientMsg::DevPauseBots { paused: true });
    roundtrip(ClientMsg::DevSetDrops {
        chances: vec![0.1; 8],
    });
    roundtrip(ClientMsg::DevResetDrops);
    roundtrip(ClientMsg::DevSetBotBehaviour {
        behaviour: serde_json::json!("aggressive"),
    });
    roundtrip(ClientMsg::DevSetBotBehaviour {
        behaviour: serde_json::Value::Null,
    });
    // a DevState from an older host has no behaviour: random
    let old = r#"{"type":"DevState","paused":false,"drops":[0,0,0,0,0,0,0,0]}"#;
    assert!(matches!(
        decode::<ServerMsg>(old.as_bytes()),
        Ok(ServerMsg::DevState {
            behaviour: None,
            ..
        })
    ));
    let old = r#"{"type":"Welcome","slot":0,"config":null}"#;
    assert!(
        decode::<ServerMsg>(old.as_bytes()).is_err(),
        "config is still required"
    );
    let mut value = serde_json::to_value(ServerMsg::Welcome {
        slot: 0,
        config: Config::default(),
        lobby: Lobby::new(),
        looks: vec![],
        dev: true,
    })
    .unwrap();
    value.as_object_mut().unwrap().remove("dev");
    match serde_json::from_value::<ServerMsg>(value).unwrap() {
        ServerMsg::Welcome { dev, .. } => assert!(!dev),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f4_old_messages_without_the_new_fields_still_decode() {
    // a Join of an old client carries neither a look nor a room
    let join: ClientMsg = decode(br#"{"type":"Join","version":1,"name":"Old"}"#).unwrap();
    assert!(matches!(
        join,
        ClientMsg::Join {
            room: None,
            look: None,
            ..
        }
    ));
    // a lobby of an old host has no settings: the defaults apply
    let lobby: ServerMsg = decode(
        br#"{"type":"Lobby","lobby":{"slots":[
            {"controller":"Remote","name":"A","ready":false},
            {"controller":"Empty","name":"","ready":false},
            {"controller":"Empty","name":"","ready":false},
            {"controller":"Empty","name":"","ready":false}]}}"#,
    )
    .unwrap();
    match lobby {
        ServerMsg::Lobby { lobby, .. } => {
            assert_eq!(lobby.settings, eksplode_re_core::RoomSettings::default());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f4_a_client_that_does_not_know_a_message_gets_an_error_not_a_panic() {
    for line in [
        r#"{"type":"RoomInfoV2","open":true}"#,
        r#"{"type":"RoomInfo"}"#,
        r#"{"type":"RoomInfo","open":"yes","settings":5,"humans":-1}"#,
        r#"{"type":"HeartLost"}"#,
        r#"{"type":"Ended","outcome":{"TeamWin":"x"}}"#,
        r#"{"type":"Ended","outcome":"NobodyWins"}"#,
    ] {
        assert!(decode::<ServerMsg>(line.as_bytes()).is_err(), "{line}");
    }
    // and the new outcomes round trip
    for outcome in [
        Outcome::TeamWin(1),
        Outcome::ChaserWins,
        Outcome::RunnersWin,
    ] {
        roundtrip(ServerMsg::Ended { outcome });
    }
    roundtrip(ServerMsg::RoomInfo {
        open: true,
        settings: eksplode_re_core::RoomSettings::default(),
        humans: 2,
    });
    roundtrip(ClientMsg::RoomQuery);
    roundtrip(ClientMsg::SetRoom {
        settings: serde_json::json!({"mode": "team"}),
    });
}

#[test]
fn f11_us5_match_statistics_ride_on_the_snapshot_and_default_for_old_hosts() {
    let mut state = GameState::new(Config::default(), 7, [true, true, false, false]).unwrap();
    state.players[0].kills = 2;
    state.players[0].bombs_placed = 9;
    state.players[1].eliminated_order = Some(1);
    let me = PlayerId::new(0).unwrap();
    let msg = ServerMsg::Snapshot {
        tick: 4,
        state: Box::new(state.observe(me)),
        events: vec![],
    };
    roundtrip(msg.clone());
    let mut value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["state"]["players"][0]["kills"], 2);
    assert_eq!(value["state"]["players"][0]["bombs_placed"], 9);
    assert_eq!(value["state"]["players"][1]["eliminated_order"], 1);
    assert!(value["state"]["players"][0]["eliminated_order"].is_null());
    // an older host sends none of the three fields
    for player in value["state"]["players"].as_array_mut().unwrap() {
        let player = player.as_object_mut().unwrap();
        for key in ["kills", "bombs_placed", "eliminated_order"] {
            player.remove(key);
        }
    }
    let ServerMsg::Snapshot { state, .. } = serde_json::from_value::<ServerMsg>(value).unwrap()
    else {
        panic!("not a snapshot");
    };
    assert_eq!(
        (
            state.players[0].kills,
            state.players[0].bombs_placed,
            state.players[1].eliminated_order
        ),
        (0, 0, None)
    );
}
