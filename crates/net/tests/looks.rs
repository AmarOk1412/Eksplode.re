#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_net::look::{Colour, Face, Gender, Head, Look};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use serde_json::{json, Value};

fn join(host: &eksplode_re_net::Host, name: &str, look: Option<Value>) -> (Raw, u8, Vec<Look>) {
    let mut raw = Raw::connect(host.addr());
    let mut msg = json!({"type": "Join", "version": 1, "name": name});
    if let Some(look) = look {
        msg["look"] = look;
    }
    raw.send_line(&msg.to_string());
    match raw.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, looks, .. }) => (raw, slot, looks),
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

fn lobby_looks(raw: &mut Raw, pred: impl Fn(&[Look]) -> bool) -> Vec<Look> {
    match raw.recv_until(|m| matches!(m, ServerMsg::Lobby { looks, .. } if pred(looks))) {
        Some(ServerMsg::Lobby { looks, .. }) => looks,
        other => panic!("no matching lobby: {other:?}"),
    }
}

#[test]
fn a_chosen_look_is_in_the_welcome_and_in_every_lobby_broadcast() {
    let host = host();
    let chosen = json!({"colour":"Green","gender":"Female","head":"Cap","face":"Glasses"});
    let (mut a, slot, looks) = join(&host, "Ann", Some(chosen));
    assert_eq!(slot, 0);
    let expected = Look {
        colour: Colour::Green,
        gender: Gender::Female,
        head: Head::Cap,
        face: Face::Glasses,
    };
    assert_eq!(looks[0], expected);
    assert_eq!(looks.len(), 4);

    let (_b, slot, _) = join(&host, "Bob", None);
    assert_eq!(slot, 1);
    let seen = lobby_looks(&mut a, |l| l[1] != Look::default_for(1) || l[0] == expected);
    assert_eq!(seen[0], expected, "Ann still has her look");
    assert_eq!(seen[1], Look::default_for(1), "Bob got his slot default");
}

#[test]
fn f3_us1_as7_a_join_without_a_look_gets_the_slot_default() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send_line(r#"{"type":"Join","version":1,"name":"Old"}"#);
    match raw.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { looks, .. }) => assert_eq!(looks[0], Look::default_for(0)),
        other => panic!("{other:?}"),
    }
    // the in-process client of the library sends no look either
    let c = connect(host.addr(), "Lib");
    assert_eq!(welcome_slot(&c), 1);
}

#[test]
fn f3_us1_as4_a_taken_colour_moves_to_the_next_free_one_and_the_sender_is_told() {
    let host = host();
    let (_a, _, _) = join(&host, "Ann", Some(json!({"colour":"Green"})));
    let (mut b, slot, looks) = join(&host, "Bob", Some(json!({"colour":"Green","head":"Crown"})));
    assert_eq!(slot, 1);
    assert_eq!(
        looks[1].colour,
        Colour::Orange,
        "the next colour after Green"
    );
    assert_eq!(looks[1].head, Head::Crown, "the rest of the look is kept");
    match b.recv_until(is_error("LookAdjusted")) {
        Some(ServerMsg::Error { message, .. }) => assert!(message.contains("Orange"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn colours_stay_distinct_among_four_participants_including_bots() {
    let host = host();
    let (mut a, _, _) = join(&host, "Ann", Some(json!({"colour":"Black"})));
    for _ in 0..3 {
        a.send(&ClientMsg::AddBot);
    }
    let looks = match a
        .recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4))
    {
        Some(ServerMsg::Lobby { looks, .. }) => looks,
        other => panic!("{other:?}"),
    };
    let mut colours: Vec<&str> = looks.iter().map(|l| l.colour.name()).collect();
    colours.sort_unstable();
    colours.dedup();
    assert_eq!(colours.len(), 4, "{looks:?}");
    assert_eq!(
        looks[0].colour,
        Colour::Black,
        "the seated human keeps their choice"
    );
}

#[test]
fn f3_us1_as6_set_look_changes_everybody_s_lobby_and_is_refused_during_a_match() {
    let host = host();
    let (mut a, _, _) = join(&host, "Ann", None);
    let (mut b, _, _) = join(&host, "Bob", None);
    a.send_line(r#"{"type":"SetLook","look":{"colour":"Teal","gender":"Male","head":"TopHat","face":"Mask"}}"#);
    for raw in [&mut a, &mut b] {
        let looks = lobby_looks(raw, |l| l[0].head == Head::TopHat);
        assert_eq!(looks[0].colour, Colour::Teal);
        assert_eq!(looks[0].face, Face::Mask);
    }
    a.send(&ClientMsg::AddBot);
    a.send(&ClientMsg::Start);
    assert!(a
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
    a.send_line(r#"{"type":"SetLook","look":{"colour":"Pink"}}"#);
    assert!(a.recv_until(is_error("WrongPhase")).is_some());
}

#[test]
fn a_look_that_changes_to_a_taken_colour_is_adjusted_too() {
    let host = host();
    let (mut a, _, _) = join(&host, "Ann", Some(json!({"colour":"Green"})));
    let (mut b, _, _) = join(&host, "Bob", Some(json!({"colour":"Pink"})));
    b.send_line(r#"{"type":"SetLook","look":{"colour":"Green"}}"#);
    assert!(b.recv_until(is_error("LookAdjusted")).is_some());
    let looks = lobby_looks(&mut a, |l| l[1].colour == Colour::Orange);
    assert_eq!(looks[0].colour, Colour::Green);
}

#[test]
fn a_freed_slot_frees_its_colour() {
    let host = host();
    let (_a, _, _) = join(&host, "Ann", Some(json!({"colour":"Green"})));
    let (mut b, _, looks) = join(&host, "Bob", Some(json!({"colour":"Pink"})));
    assert_eq!(looks[1].colour, Colour::Pink);
    b.send(&ClientMsg::Leave);
    drop(b);
    // Bob's colour is free again for the next joiner
    let (_c, slot, looks) = join(&host, "Cy", Some(json!({"colour":"Pink"})));
    assert_eq!(slot, 1);
    assert_eq!(looks[1].colour, Colour::Pink);
}

#[test]
fn hostile_looks_never_break_the_join() {
    let host = host();
    let hostile = [
        json!(7),
        json!("Blue"),
        json!({"colour": {"x": 1}, "head": [1], "face": 3.5}),
        json!(null),
    ];
    let mut raws = Vec::new();
    for (i, look) in hostile.into_iter().enumerate() {
        let (raw, slot, looks) = join(&host, &format!("H{i}"), Some(look));
        assert_eq!(usize::from(slot), i);
        assert_eq!(
            looks[i],
            Look::default_for(i),
            "a hostile look becomes the default"
        );
        raws.push(raw);
    }
}
