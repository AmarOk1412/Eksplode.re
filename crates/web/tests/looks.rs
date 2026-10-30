//! Looks over the WebSocket: every tab sees the others' looks, and no look can hurt anybody.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_net::look::{Colour, Costume, Face, Head, Look};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use serde_json::json;

fn welcome_looks(ws: &mut RoomWs) -> Vec<Look> {
    match ws.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { looks, .. }) => looks,
        other => panic!("expected Welcome, got {other:?}"),
    }
}

#[test]
fn two_tabs_see_each_other_s_looks_in_welcome_and_lobby() {
    let s = server(|_| {});
    let mut a = RoomWs::create_with_look(
        s.http_addr(),
        "Ann",
        // 011: an old page still sends a gender; it is ignored
        Some(
            json!({"colour":"Green","gender":"Female","head":"Crown","face":"Moustache","costume":"Medal"}),
        ),
    );
    let looks = welcome_looks(&mut a);
    let ann = Look {
        colour: Colour::Green,
        head: Head::Crown,
        face: Face::Moustache,
        costume: Costume::Medal,
    };
    assert_eq!(looks[0], ann);

    let mut b = RoomWs::join_with_look(
        s.http_addr(),
        &a.reference,
        "Bob",
        Some(json!({"colour":"Pink","head":"Cap"})),
    );
    let seen_by_bob = welcome_looks(&mut b);
    assert_eq!(seen_by_bob[0], ann, "Bob sees Ann's look");
    assert_eq!(seen_by_bob[1].colour, Colour::Pink);

    let lobby = a.recv_until(
        |m| matches!(m, ServerMsg::Lobby { looks, .. } if looks[1].colour == Colour::Pink),
    );
    match lobby {
        Some(ServerMsg::Lobby { looks, .. }) => {
            assert_eq!(looks[1].head, Head::Cap);
            assert_eq!(looks[0], ann);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f3_us1_as6_a_look_change_over_websocket_reaches_the_other_tab() {
    let s = server(|_| {});
    let mut a = RoomWs::create(s.http_addr(), "Ann");
    welcome_looks(&mut a);
    let mut b = RoomWs::join(s.http_addr(), &a.reference, "Bob");
    welcome_looks(&mut b);
    a.send(&ClientMsg::SetLook {
        look: json!({"colour":"Teal","gender":"Male","head":"Headphones","face":"Glasses"}),
    });
    let got = b.recv_until(
        |m| matches!(m, ServerMsg::Lobby { looks, .. } if looks[0].head == Head::Headphones),
    );
    match got {
        Some(ServerMsg::Lobby { looks, .. }) => {
            assert_eq!(looks[0].colour, Colour::Teal);
            assert_eq!(looks[0].face, Face::Glasses);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile_looks_in_text_frames_never_break_anybody() {
    let s = server(|_| {});
    let mut good = RoomWs::create(s.http_addr(), "Ann");
    welcome_looks(&mut good);
    let hostile = [
        json!(7),
        json!({"colour":{"x":[1,2,3]},"head":null}),
        json!("Blue"),
    ];
    for (i, look) in hostile.into_iter().enumerate() {
        let mut bad = Ws::open(s.http_addr());
        bad.hello_v2().unwrap();
        bad.send_value(&json!({"type":"JoinRoom", "request_id":"hostile-join", "name":format!("M{i}"),
            "room_id":good.reference.room_id, "server_epoch":good.reference.server_epoch, "look":look}));
        let welcome = bad.recv_value().unwrap();
        assert_eq!(welcome["type"], "Welcome");
        let slot = welcome["slot"].as_u64().unwrap() as usize;
        let looks: Vec<Look> = serde_json::from_value(welcome["looks"].clone()).unwrap();
        assert_eq!(looks.len(), 4);
        assert_eq!(looks[slot], Look::default_for(slot));
        for (index, look) in [json!([[[]]]), json!("z".repeat(5000))]
            .into_iter()
            .enumerate()
        {
            bad.send_value(
                &json!({"type":"SetLook", "request_id":format!("hostile-look-{index}"),
                "room_id":good.reference.room_id, "look":look}),
            );
        }
    }
    // the first player is untouched and the server still plays
    good.send(&ClientMsg::AddBot);
    good.send(&ClientMsg::Start);
    assert!(good
        .recv_until(|m| matches!(m, ServerMsg::Snapshot { .. }))
        .is_some());
}

#[test]
fn a_taken_colour_over_websocket_is_adjusted_with_a_notice() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.hello_v2().unwrap();
    a.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Ann", "capacity":4,
        "settings":eksplode_re_core::RoomSettings::default(), "look":{"colour":"Yellow"}}),
    );
    let first = a.recv_value().unwrap();
    assert_eq!(first["type"], "Welcome");
    let mut b = Ws::open(s.http_addr());
    b.hello_v2().unwrap();
    b.send_value(&json!({"type":"JoinRoom", "request_id":"join", "name":"Bob",
        "room_id":first["room_id"], "server_epoch":first["server_epoch"], "look":{"colour":"Yellow"}}));
    let looks: Vec<Look> =
        serde_json::from_value(b.recv_value().unwrap()["looks"].clone()).unwrap();
    assert_eq!(looks[1].colour, Colour::Pink);
    let notice =
        b.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "LookAdjusted"));
    assert!(notice.is_some());
    b.send_value(
        &json!({"type":"SetLook", "request_id":"change", "room_id":first["room_id"],
        "look":{"colour":"Yellow", "head":"Crown"}}),
    );
    assert!(b
        .recv_until(|message| matches!(message,
        ServerMsg::Error { code, message } if code == "LookAdjusted" && message.contains("Pink")))
        .is_some());
    a.send_value(&json!({"type":"Ping", "nonce":77, "room_id":first["room_id"]}));
    loop {
        let value = a.recv_value().unwrap();
        assert_ne!(
            value["code"], "LookAdjusted",
            "the notice belongs only to its sender"
        );
        if value["type"] == "Pong" {
            break;
        }
    }
    let directory: serde_json::Value =
        serde_json::from_slice(&get(s.http_addr(), "/rooms").body).unwrap();
    assert_eq!(directory["rooms"][0]["connected_humans"], 2);
}
