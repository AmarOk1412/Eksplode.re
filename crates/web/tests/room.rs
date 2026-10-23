//! Room settings over the WebSocket: the first tab creates the room, the others see it, and no
//! settings value can hurt anybody.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use bomberust_core::{BoardSize, RoomSettings};
use bomberust_net::protocol::ServerMsg;
use common::*;
use serde_json::json;
use tungstenite::Message;

fn welcome_settings(ws: &mut Ws) -> RoomSettings {
    match ws.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { lobby, .. }) => lobby.settings,
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn room_info(ws: &mut Ws) -> (bool, RoomSettings, u8) {
    ws.send_text(r#"{"type":"RoomQuery"}"#);
    match ws.recv_until(|m| matches!(m, ServerMsg::RoomInfo { .. })) {
        Some(ServerMsg::RoomInfo {
            open,
            settings,
            humans,
        }) => (open, settings, humans),
        other => panic!("expected RoomInfo, got {other:?}"),
    }
}

#[test]
fn two_tabs_see_the_same_settings() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    let (open, defaults, humans) = room_info(&mut a);
    assert!(!open);
    assert_eq!(defaults, RoomSettings::default());
    assert_eq!(humans, 0);
    a.join_with_room(
        "Ann",
        json!({"name": "Friday", "size": "medium", "minutes": 5}),
    );
    let first = welcome_settings(&mut a);
    assert_eq!(first.name, "Friday");
    assert_eq!(first.size, BoardSize::Medium);

    let mut b = Ws::open(s.http_addr());
    let (open, seen, humans) = room_info(&mut b);
    assert!(open);
    assert_eq!(seen, first);
    assert_eq!(humans, 1);
    b.join("Bob");
    assert_eq!(welcome_settings(&mut b), first);

    a.send_text(r#"{"type":"SetRoom","settings":{"name":"Renamed","size":"small"}}"#);
    let want = |m: &ServerMsg| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.settings.name == "Renamed");
    assert!(a.recv_until(want).is_some());
    match b.recv_until(want) {
        Some(ServerMsg::Lobby { lobby, .. }) => {
            assert_eq!(lobby.settings.size, BoardSize::Small);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile_settings_never_produce_malformed_and_never_affect_the_other_tab() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.join("Ann");
    let ann = welcome_settings(&mut a);

    let mut b = Ws::open(s.http_addr());
    b.join_with_room(
        "Bob",
        json!({"name": "N".repeat(60_000), "mode": [1], "size": {"x": null}, "minutes": 1e300,
               "friendly_fire": [], "deep": [[[[[[[[1]]]]]]]]}),
    );
    // Bob joined an open room: his room value is ignored and he is not refused
    assert_eq!(welcome_settings(&mut b), ann);

    for text in [
        r#"{"type":"SetRoom","settings":5}"#.to_string(),
        r#"{"type":"SetRoom","settings":null}"#.to_string(),
        r#"{"type":"SetRoom"}"#.to_string(),
        format!(
            r#"{{"type":"SetRoom","settings":{{"name":"{}","minutes":-1}}}}"#,
            "é".repeat(20_000)
        ),
    ] {
        b.socket.send(Message::text(text)).unwrap();
    }
    // Ann is untouched and still sees a sane room
    let (open, seen, humans) = room_info(&mut a);
    assert!(open);
    assert_eq!(seen, ann);
    assert_eq!(humans, 2);
}

#[test]
fn a_long_hostile_room_in_the_creating_join_is_cleaned() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.join_with_room(
        "Ann",
        json!({"name": "\u{7}x".repeat(5_000), "minutes": 77, "size": "huge"}),
    );
    let got = welcome_settings(&mut a);
    assert_eq!(got.name.chars().count(), 24);
    assert_eq!(got.minutes, 3);
    assert_eq!(got.size, BoardSize::Small);
}
