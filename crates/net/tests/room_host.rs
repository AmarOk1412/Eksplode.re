#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use bomberust_core::{BoardSize, Lobby, Mode, RoomSettings};
use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_net::Host;
use common::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn join(host: &Host, name: &str, room: Option<Value>) -> (Raw, u8, Lobby) {
    let mut raw = Raw::connect(host.addr());
    let mut msg = json!({"type": "Join", "version": 1, "name": name});
    if let Some(room) = room {
        msg["room"] = room;
    }
    raw.send_line(&msg.to_string());
    match raw.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, lobby, .. }) => (raw, slot, lobby),
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn room_info(raw: &mut Raw) -> (bool, RoomSettings, u8) {
    raw.send_line(r#"{"type":"RoomQuery"}"#);
    match raw.recv_until(|m| matches!(m, ServerMsg::RoomInfo { .. })) {
        Some(ServerMsg::RoomInfo {
            open,
            settings,
            humans,
        }) => (open, settings, humans),
        other => panic!("expected RoomInfo, got {other:?}"),
    }
}

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

fn lobby_settings(raw: &mut Raw, pred: impl Fn(&RoomSettings) -> bool) -> RoomSettings {
    match raw.recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if pred(&lobby.settings))) {
        Some(ServerMsg::Lobby { lobby, .. }) => lobby.settings,
        other => panic!("no matching lobby: {other:?}"),
    }
}

#[test]
fn f4_us1_as1_a_closed_room_answers_room_info_and_the_first_join_creates_it() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    let (open, settings, humans) = room_info(&mut raw);
    assert!(!open);
    assert_eq!(settings, RoomSettings::default());
    assert_eq!(humans, 0);
    // asking again changes nothing and takes no slot
    assert!(!room_info(&mut raw).0);

    let room = json!({"name": "Friday", "size": "medium", "minutes": 5});
    raw.send_line(&json!({"type":"Join","version":1,"name":"Ann","room":room}).to_string());
    let lobby = match raw.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, lobby, .. }) => {
            assert_eq!(slot, 0, "the query took no slot");
            lobby
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(lobby.settings.name, "Friday");
    assert_eq!(lobby.settings.size, BoardSize::Medium);
    assert_eq!(lobby.settings.minutes, 5);
    assert_eq!(lobby.owner(), Some(0));

    let (open, settings, humans) = room_info(&mut raw);
    assert!(open);
    assert_eq!(settings.name, "Friday");
    assert_eq!(humans, 1);
}

#[test]
fn the_first_join_without_a_room_gets_the_defaults_and_a_name_from_the_owner() {
    let host = host();
    let (_a, _, lobby) = join(&host, "Ann", None);
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
    let (open, settings, _) = room_info(&mut raw);
    assert!(!open);
    assert_eq!(settings.size, BoardSize::Large);
    let (_a, _, lobby) = join(&host, "Ann", None);
    assert_eq!(lobby.settings.name, "Lab");
    assert_eq!(lobby.settings.minutes, 10);
}

#[test]
fn f4_us1_as2_a_second_player_joins_the_open_room_and_cannot_change_it() {
    let host = host();
    let room = json!({"name": "Friday", "minutes": 5});
    let (_a, _, _) = join(&host, "Ann", Some(room));
    let other = json!({"name": "Mine", "size": "large", "minutes": 10});
    let (mut b, slot, lobby) = join(&host, "Bob", Some(other));
    assert_eq!(slot, 1);
    assert_eq!(
        lobby.settings.name, "Friday",
        "Join.room is ignored in an open room"
    );
    assert_eq!(lobby.settings.minutes, 5);
    assert_eq!(lobby.settings.size, BoardSize::Small);

    b.send_line(r#"{"type":"SetRoom","settings":{"minutes":2}}"#);
    assert!(b.recv_until(is_error("NotOwner")).is_some());
    let (_, settings, humans) = room_info(&mut b);
    assert_eq!(settings.minutes, 5);
    assert_eq!(humans, 2);
}

#[test]
fn f4_us1_as3_the_owner_changes_the_settings_and_everybody_sees_them() {
    let host = host_with(|c| c.game = quick_game(30));
    let (mut a, _, _) = join(&host, "Ann", None);
    let (mut b, _, _) = join(&host, "Bob", None);
    a.send_line(
        r#"{"type":"SetRoom","settings":{"name":"New name","size":"large","minutes":10,"mode":"battle"}}"#,
    );
    let want = |s: &RoomSettings| s.name == "New name";
    for raw in [&mut a, &mut b] {
        let s = lobby_settings(raw, want);
        assert_eq!(s.size, BoardSize::Large);
        assert_eq!(s.minutes, 10);
        assert_eq!(s.mode, Mode::Normal, "an unknown mode becomes the default");
    }
    // the next match uses them
    a.send(&ClientMsg::Start);
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
    let (mut a, _, _) = join(&host, "Ann", None);
    a.send_line(r#"{"type":"SetRoom","settings":5}"#);
    assert!(a.recv_until(is_error("BadRoom")).is_some());
    a.send_line(r#"{"type":"SetRoom","settings":"team"}"#);
    assert!(a.recv_until(is_error("BadRoom")).is_some());
    let (_, settings, _) = room_info(&mut a);
    assert_eq!(settings.name, "Ann's room");
}

#[test]
fn f4_us1_as4_settings_are_refused_during_a_match_and_kept_for_the_rematch() {
    let host = host_with(|c| c.game = quick_game(400));
    let room = json!({"name": "Keep me", "size": "medium"});
    let (mut a, _, _) = join(&host, "Ann", Some(room));
    a.send(&ClientMsg::Start);
    assert!(a
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
    a.send_line(r#"{"type":"SetRoom","settings":{"name":"Too late"}}"#);
    assert!(a.recv_until(is_error("WrongPhase")).is_some());

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
    let room = json!({"name": "Gone soon", "size": "large"});
    let (a, _, _) = join(&host, "Ann", Some(room));
    drop(a);

    let mut probe = Raw::connect(host.addr());
    let end = Instant::now() + WAIT;
    loop {
        let (open, settings, humans) = room_info(&mut probe);
        if !open {
            assert_eq!(settings, RoomSettings::default());
            assert_eq!(humans, 0);
            break;
        }
        assert!(Instant::now() < end, "the room never closed");
        std::thread::sleep(Duration::from_millis(20));
    }
    let (_b, slot, lobby) = join(&host, "Bob", None);
    assert_eq!(slot, 0);
    assert_eq!(lobby.settings.size, BoardSize::Small);
    assert_eq!(lobby.settings.name, "Bob's room");
}

#[test]
fn an_old_client_sees_the_same_message_sequence() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send_line(r#"{"type":"Join","version":1,"name":"Old"}"#);
    assert!(matches!(raw.recv(), Some(ServerMsg::Welcome { .. })));
    assert!(matches!(raw.recv(), Some(ServerMsg::Lobby { .. })));
}

#[test]
fn a_hostile_room_never_breaks_the_join() {
    let host = host();
    let hostile = json!({
        "name": "A".repeat(50_000), "mode": {"x": 1}, "size": [[]], "minutes": 1e30,
        "friendly_fire": null, "junk": {"a": {"b": {"c": [1, 2, 3]}}}
    });
    let (_a, slot, lobby) = join(&host, "Ann", Some(hostile));
    assert_eq!(slot, 0);
    assert_eq!(lobby.settings.name.chars().count(), 24);
    assert_eq!(lobby.settings.minutes, 3);
    assert_eq!(lobby.settings.size, BoardSize::Small);
}
