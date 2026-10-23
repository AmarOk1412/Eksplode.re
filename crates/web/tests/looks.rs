//! Looks over the WebSocket: every tab sees the others' looks, and no look can hurt anybody.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use bomberust_net::look::{Colour, Face, Gender, Head, Look};
use bomberust_net::protocol::{ClientMsg, ServerMsg};
use common::*;
use serde_json::json;

fn welcome_looks(ws: &mut Ws) -> Vec<Look> {
    match ws.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { looks, .. }) => looks,
        other => panic!("expected Welcome, got {other:?}"),
    }
}

#[test]
fn two_tabs_see_each_other_s_looks_in_welcome_and_lobby() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.join_with_look(
        "Ann",
        json!({"colour":"Green","gender":"Female","head":"Crown","face":"Moustache"}),
    );
    let looks = welcome_looks(&mut a);
    let ann = Look {
        colour: Colour::Green,
        gender: Gender::Female,
        head: Head::Crown,
        face: Face::Moustache,
    };
    assert_eq!(looks[0], ann);

    let mut b = Ws::open(s.http_addr());
    b.join_with_look("Bob", json!({"colour":"Pink","head":"Cap"}));
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
    let mut a = Ws::open(s.http_addr());
    a.join("Ann");
    welcome_looks(&mut a);
    let mut b = Ws::open(s.http_addr());
    b.join("Bob");
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
    let mut good = Ws::open(s.http_addr());
    good.join("Ann");
    welcome_looks(&mut good);
    let hostile = [
        r#"{"type":"Join","version":1,"name":"M1","look":7}"#,
        r#"{"type":"Join","version":1,"name":"M2","look":{"colour":{"x":[1,2,3]},"head":null}}"#,
        r#"{"type":"Join","version":1,"name":"M3","look":"Blue"}"#,
    ];
    for (i, text) in hostile.into_iter().enumerate() {
        let mut bad = Ws::open(s.http_addr());
        bad.send_text(text);
        let looks = welcome_looks(&mut bad);
        assert_eq!(looks.len(), 4, "{text}");
        assert_eq!(looks[i + 1], Look::default_for(i + 1), "{text}");
        bad.send_text(r#"{"type":"SetLook","look":[[[]]]}"#);
        bad.send_text(&format!(
            r#"{{"type":"SetLook","look":"{}"}}"#,
            "z".repeat(5000)
        ));
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
    a.join_with_look("Ann", json!({"colour":"Yellow"}));
    welcome_looks(&mut a);
    let mut b = Ws::open(s.http_addr());
    b.join_with_look("Bob", json!({"colour":"Yellow"}));
    let looks = welcome_looks(&mut b);
    assert_eq!(looks[1].colour, Colour::Pink);
    let notice =
        b.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "LookAdjusted"));
    assert!(notice.is_some());
}
