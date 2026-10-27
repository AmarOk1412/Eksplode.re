#![allow(clippy::unwrap_used)]

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
