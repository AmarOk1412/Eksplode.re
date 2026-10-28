mod common;

use std::time::Duration;

use common::*;
use eksplode_re_core::{Config, Dir, Event};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

#[test]
fn v2_websocket_negotiates_and_preserves_scoped_command_identity() {
    let server = server(|options| {
        options.server_id = Some("alpha".into());
        options.room.minutes = 5;
    });
    let mut socket = Ws::open(server.http_addr());
    let hello = socket.hello_v2().unwrap();
    assert_eq!(hello["type"], "Hello");
    assert_eq!(hello["version"], 2);
    assert_eq!(hello["server_id"], "alpha");
    assert_eq!(hello["defaults"]["minutes"], 5);
    socket.send_value(&serde_json::json!({
        "type": "Start", "request_id": "start", "room_id": "missing-room"
    }));
    let reply = socket.recv_value().unwrap();
    assert_eq!(reply["type"], "Error");
    assert_eq!(reply["request_id"], "start");
    assert_eq!(reply["code"], "NotInRoom");
}

#[test]
fn us1_as2_join_assigns_a_slot_and_a_fifth_join_is_refused() {
    let s = server(|_| {});
    let four = join_room_ws(s.http_addr(), 4);
    let mut late = Ws::open(s.http_addr());
    late.hello_v2().unwrap();
    late.join_room_v2(
        "late",
        &serde_json::to_value(&four[0].reference).unwrap(),
        "late",
    );
    assert_eq!(late.recv_value().unwrap()["code"], "Full");
}

#[test]
fn invalid_names_and_versions_are_rejected() {
    let s = server(|_| {});
    let owner = RoomWs::create(s.http_addr(), "Owner");
    for name in ["", "   ", "this name is far too long", "bad\u{7}name"] {
        let mut ws = Ws::open(s.http_addr());
        ws.hello_v2().unwrap();
        ws.join_room_v2(
            "invalid-name",
            &serde_json::to_value(&owner.reference).unwrap(),
            name,
        );
        assert_eq!(ws.recv_value().unwrap()["code"], "BadName", "{name:?}");
    }
    let mut ws = Ws::open(s.http_addr());
    ws.send_value(&serde_json::json!({"type":"Hello", "version":99}));
    assert_eq!(
        ws.recv_value().unwrap(),
        serde_json::json!({"type":"Rejected", "reason":"BadVersion"})
    );
}

#[test]
fn room_commands_must_follow_hello() {
    let s = server(|_| {});
    let mut ws = Ws::open(s.http_addr());
    ws.send_value(
        &serde_json::json!({"type":"Start", "request_id":"before-hello", "room_id":"unknown"}),
    );
    let m = ws.recv_until(is_error("BadVersion"));
    assert!(m.is_some());
}

#[test]
fn owner_start_over_websocket() {
    let s = server(|_| {});
    let mut owner = RoomWs::create(s.http_addr(), "Ann");
    assert_eq!(owner.welcome_slot(), 0);
    owner.send(&ClientMsg::AddBot);
    owner.send(&ClientMsg::Start);
    let start = owner.recv_until(|m| matches!(m, ServerMsg::Start { .. }));
    assert!(matches!(start, Some(ServerMsg::Start { seed: 5, .. })));
    assert!(owner
        .recv_until(|m| matches!(m, ServerMsg::Snapshot { .. }))
        .is_some());
}

#[test]
fn a_non_owner_gets_notowner() {
    let s = server(|_| {});
    let mut ws = join_room_ws(s.http_addr(), 2);
    ws[1].send(&ClientMsg::Start);
    let e = ws[1].recv_until(is_error("NotOwner"));
    match e {
        Some(ServerMsg::Error { message, .. }) => assert!(message.contains("w0"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn us1_as4_input_moves_the_player() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    ws.welcome_slot();
    ws.send(&ClientMsg::AddBot);
    ws.send(&ClientMsg::Start);
    let mut tick = 0;
    let mut moved = false;
    for _ in 0..400 {
        ws.send(&ClientMsg::Input {
            tick,
            mv: Some(Dir::Down),
            bomb: false,
            punch: false,
        });
        match ws.recv() {
            Some(ServerMsg::Snapshot {
                tick: t, events, ..
            }) => {
                tick = t;
                moved |= events
                    .iter()
                    .any(|e| matches!(e, Event::Moved { player, .. } if player.index() == 0));
            }
            Some(_) => {}
            None => break,
        }
        if moved {
            break;
        }
    }
    assert!(moved, "the character never moved");
}

#[test]
fn us1_as5_the_result_is_announced_and_the_lobby_returns() {
    let s = server(|o| {
        o.game = Config {
            block_density: 0.0,
            round_limit_ticks: 40,
            ..Config::default()
        };
    });
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    ws.welcome_slot();
    ws.send(&ClientMsg::AddBot);
    ws.send(&ClientMsg::Start);
    assert!(ws
        .recv_until(|m| matches!(m, ServerMsg::Ended { .. }))
        .is_some());
    assert!(ws
        .recv_until(|m| matches!(m, ServerMsg::Lobby { .. }))
        .is_some());
    // and a rematch works
    ws.send(&ClientMsg::Start);
    assert!(ws
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
}

#[test]
fn binary_frames_are_malformed() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    ws.welcome_slot();
    ws.socket
        .send(tungstenite::Message::binary(vec![1u8, 2, 3]))
        .unwrap();
    assert!(ws.recv_until(is_error("Malformed")).is_some());
    // the connection still works
    ws.send(&ClientMsg::Ping { nonce: 4 });
    assert!(ws
        .recv_until(|m| matches!(m, ServerMsg::Pong { nonce: 4 }))
        .is_some());
}

#[test]
fn malformed_text_is_answered_and_counted_ten_closes_the_socket() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    ws.welcome_slot();
    for text in [
        "garbage",
        "{\"type\":\"Nope\"}",
        "{\"type\":\"Input\",\"tick\":\"x\"}",
    ] {
        ws.send_text(text);
        assert!(ws.recv_until(is_error("Malformed")).is_some(), "{text}");
    }
    for _ in 0..7 {
        ws.send_text("garbage");
    }
    assert_eq!(ws.close_code(), Some(1008));
}

#[test]
fn a_message_over_64_kib_closes_the_socket() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    ws.welcome_slot();
    ws.send_text(&"a".repeat(70_000));
    let code = ws.close_code();
    assert!(matches!(code, Some(1009) | None), "{code:?}");
}

#[test]
fn other_players_are_not_disturbed_by_a_misbehaving_one() {
    let s = server(|_| {});
    let mut good = RoomWs::create(s.http_addr(), "Ann");
    good.welcome_slot();
    let mut bad = RoomWs::join(s.http_addr(), &good.reference, "Mal");
    bad.welcome_slot();
    for _ in 0..12 {
        bad.send_text("garbage");
    }
    good.send(&ClientMsg::AddBot);
    good.send(&ClientMsg::Start);
    assert!(good
        .recv_until(|m| matches!(m, ServerMsg::Snapshot { .. }))
        .is_some());
}

#[test]
fn websocket_pings_are_answered() {
    let s = server(|_| {});
    let mut ws = Ws::open(s.http_addr());
    ws.socket
        .send(tungstenite::Message::Ping(vec![9u8].into()))
        .unwrap();
    ws.hello_v2().unwrap();
    ws.socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    ws.send_value(&serde_json::json!({"type":"Ping", "nonce":9, "request_id":"keepalive"}));
    assert!(ws
        .recv_until(|message| matches!(message, ServerMsg::Pong { nonce: 9 }))
        .is_some());
}
