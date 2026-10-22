#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use bomberust_core::{Config, Controller, Dir, Event, GameState, Lobby, Outcome, PlayerId, Pos};
use bomberust_net::codec::{decode, encode, LineReader, NetError};
use bomberust_net::protocol::{valid_name, ClientMsg, RejectReason, ServerMsg, MAX_LINE};

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
    });
    roundtrip(ClientMsg::SetReady { ready: true });
    roundtrip(ClientMsg::Input {
        tick: 7,
        mv: Some(Dir::Left),
        bomb: true,
    });
    roundtrip(ClientMsg::Input {
        tick: 0,
        mv: None,
        bomb: false,
    });
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
        slot: 1,
        config: Config::default(),
        lobby: lobby.clone(),
    });
    roundtrip(ServerMsg::Rejected {
        reason: RejectReason::Full,
    });
    roundtrip(ServerMsg::Lobby { lobby });
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
