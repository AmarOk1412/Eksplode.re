#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_net::look::{Colour, Face, Gender, Head, Look};
use eksplode_re_net::protocol::v2::{Reply, Request, RequestId, RoomRef};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::Client;
use serde_json::{json, Value};

fn admit(
    host: &eksplode_re_net::Host,
    reference: Option<&RoomRef>,
    name: &str,
    look: Option<Value>,
) -> (Client, u8, RoomRef, Vec<Look>) {
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let request_id = RequestId::new("appearance-admission").unwrap();
    let request = match reference {
        Some(reference) => Request::JoinRoom {
            request_id,
            room_id: reference.room_id.clone(),
            server_epoch: reference.server_epoch.clone(),
            name: name.into(),
            password: None,
            look,
        },
        None => Request::CreateRoom {
            request_id,
            name: name.into(),
            settings: serde_json::to_value(eksplode_re_core::RoomSettings::default()).unwrap(),
            capacity: 4,
            password: None,
            look,
        },
    };
    client.send_v2(request).unwrap();
    let welcome = wait_for(&client, |message| {
        matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), Reply::Welcome { .. }))
    })
    .unwrap();
    match welcome {
        ServerMsg::RoomV2(reply) => match *reply {
            Reply::Welcome { admission, .. } => {
                (client, admission.slot, admission.reference, admission.looks)
            }
            _ => panic!("expected Welcome"),
        },
        _ => panic!("expected v2 reply"),
    }
}

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

fn lobby_looks(client: &Client, pred: impl Fn(&[Look]) -> bool) -> Vec<Look> {
    match wait_for_game(
        client,
        |m| matches!(m, ServerMsg::Lobby { looks, .. } if pred(looks)),
    ) {
        Some(ServerMsg::Lobby { looks, .. }) => looks,
        other => panic!("no matching lobby: {other:?}"),
    }
}

#[test]
fn a_chosen_look_is_in_the_welcome_and_in_every_lobby_broadcast() {
    let host = host();
    let chosen = json!({"colour":"Green","gender":"Female","head":"Cap","face":"Glasses"});
    let (a, slot, room, looks) = admit(&host, None, "Ann", Some(chosen));
    assert_eq!(slot, 0);
    let expected = Look {
        colour: Colour::Green,
        gender: Gender::Female,
        head: Head::Cap,
        face: Face::Glasses,
    };
    assert_eq!(looks[0], expected);
    assert_eq!(looks.len(), 4);

    let (_b, slot, _, _) = admit(&host, Some(&room), "Bob", None);
    assert_eq!(slot, 1);
    let seen = lobby_looks(&a, |l| l[1] != Look::default_for(1) || l[0] == expected);
    assert_eq!(seen[0], expected, "Ann still has her look");
    assert_eq!(seen[1], Look::default_for(1), "Bob got his slot default");
}

#[test]
fn f3_us1_as7_a_join_without_a_look_gets_the_slot_default() {
    let host = host();
    let (_owner, _, room, looks) = admit(&host, None, "Ann", None);
    assert_eq!(looks[0], Look::default_for(0));
    // the in-process client of the library sends no look either
    let c = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    c.join_room(&room, "Lib").unwrap();
    assert_eq!(welcome_slot(&c), 1);
}

#[test]
fn f3_us1_as4_a_taken_colour_moves_to_the_next_free_one_and_the_sender_is_told() {
    let host = host();
    let (_a, _, room, _) = admit(&host, None, "Ann", Some(json!({"colour":"Green"})));
    let (b, slot, _, looks) = admit(
        &host,
        Some(&room),
        "Bob",
        Some(json!({"colour":"Green","head":"Crown"})),
    );
    assert_eq!(slot, 1);
    assert_eq!(
        looks[1].colour,
        Colour::Orange,
        "the next colour after Green"
    );
    assert_eq!(looks[1].head, Head::Crown, "the rest of the look is kept");
    match wait_for_game(&b, is_error("LookAdjusted")) {
        Some(ServerMsg::Error { message, .. }) => assert!(message.contains("Orange"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn colours_stay_distinct_among_four_participants_including_bots() {
    let host = host();
    let a = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let request_id = eksplode_re_net::protocol::v2::RequestId::new("create").unwrap();
    a.send_v2(eksplode_re_net::protocol::v2::Request::CreateRoom {
        request_id,
        name: "Ann".into(),
        settings: serde_json::to_value(eksplode_re_core::RoomSettings::default()).unwrap(),
        capacity: 4,
        password: None,
        look: Some(json!({"colour":"Black"})),
    })
    .unwrap();
    welcome_slot(&a);
    for _ in 0..3 {
        a.send(ClientMsg::AddBot).unwrap();
    }
    let looks = match wait_for_game(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4),
    ) {
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
    let (a, _, room, _) = admit(&host, None, "Ann", None);
    let (b, _, _, _) = admit(&host, Some(&room), "Bob", None);
    a.send(ClientMsg::SetLook {
        look: json!({"colour":"Teal","gender":"Male","head":"TopHat","face":"Mask"}),
    })
    .unwrap();
    for client in [&a, &b] {
        let looks = lobby_looks(client, |l| l[0].head == Head::TopHat);
        assert_eq!(looks[0].colour, Colour::Teal);
        assert_eq!(looks[0].face, Face::Mask);
    }
    a.send(ClientMsg::AddBot).unwrap();
    a.send(ClientMsg::Start).unwrap();
    assert!(wait_for_game(&a, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    a.send(ClientMsg::SetLook {
        look: json!({"colour":"Pink"}),
    })
    .unwrap();
    assert!(wait_for_game(&a, is_error("InMatch")).is_some());
}

#[test]
fn a_look_that_changes_to_a_taken_colour_is_adjusted_too() {
    let host = host();
    let (a, _, room, _) = admit(&host, None, "Ann", Some(json!({"colour":"Green"})));
    let (b, _, _, _) = admit(&host, Some(&room), "Bob", Some(json!({"colour":"Pink"})));
    b.send(ClientMsg::SetLook {
        look: json!({"colour":"Green"}),
    })
    .unwrap();
    assert!(wait_for_game(&b, is_error("LookAdjusted")).is_some());
    let looks = lobby_looks(&a, |l| l[1].colour == Colour::Orange);
    assert_eq!(looks[0].colour, Colour::Green);
}

#[test]
fn a_freed_slot_frees_its_colour() {
    let host = host();
    let (a, _, room, _) = admit(&host, None, "Ann", Some(json!({"colour":"Green"})));
    let (b, _, _, looks) = admit(&host, Some(&room), "Bob", Some(json!({"colour":"Pink"})));
    assert_eq!(looks[1].colour, Colour::Pink);
    b.send(ClientMsg::Leave).unwrap();
    drop(b);
    wait_for_game(&a, |message| {
        matches!(message, ServerMsg::Lobby { lobby, .. }
        if lobby.slots[1].controller == eksplode_re_core::Controller::Empty)
    })
    .unwrap();
    // Bob's colour is free again for the next joiner
    let (_c, slot, _, looks) = admit(&host, Some(&room), "Cy", Some(json!({"colour":"Pink"})));
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
    let mut room = None;
    for (i, look) in hostile.into_iter().enumerate() {
        let (raw, slot, reference, looks) =
            admit(&host, room.as_ref(), &format!("H{i}"), Some(look));
        room = Some(reference);
        assert_eq!(usize::from(slot), i);
        assert_eq!(
            looks[i],
            Look::default_for(i),
            "a hostile look becomes the default"
        );
        raws.push(raw);
    }
}
