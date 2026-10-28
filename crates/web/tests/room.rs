//! Explicit room settings over WebSocket and atomic rejection of hostile input.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::RoomSettings;
use serde_json::json;

fn reply(ws: &mut Ws, kind: &str) -> serde_json::Value {
    let deadline = std::time::Instant::now() + WAIT;
    while std::time::Instant::now() < deadline {
        let value = ws.recv_value().expect("room reply before timeout");
        if value["type"] == kind {
            return value;
        }
    }
    panic!("expected {kind}");
}

fn rooms(ws: &mut Ws) -> Vec<serde_json::Value> {
    ws.send_value(&json!({"type":"ListRooms", "request_id":"directory", "scope":"local"}));
    reply(ws, "Rooms")["rooms"].as_array().unwrap().clone()
}

#[test]
fn two_tabs_see_the_same_settings() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    let hello = a.hello_v2().unwrap();
    assert_eq!(
        hello["defaults"],
        serde_json::to_value(RoomSettings::default()).unwrap()
    );
    assert!(rooms(&mut a).is_empty());
    let mut settings = hello["defaults"].clone();
    settings["name"] = json!("Friday");
    settings["size"] = json!("medium");
    settings["minutes"] = json!(5);
    a.send_value(
        &json!({"type":"CreateRoom", "request_id":"create", "name":"Ann",
        "settings":settings, "capacity":4}),
    );
    let first = reply(&mut a, "Welcome");
    assert_eq!(first["lobby"]["settings"], settings);

    let mut b = Ws::open(s.http_addr());
    assert!(b.hello_v2().is_some());
    let directory = rooms(&mut b);
    assert_eq!(directory.len(), 1);
    assert_eq!(directory[0]["room_id"], first["room_id"]);
    assert_eq!(directory[0]["name"], "Friday");
    assert_eq!(directory[0]["connected_humans"], 1);
    b.join_room_v2("join", &first, "Bob");
    let joined = reply(&mut b, "Welcome");
    assert_eq!(joined["lobby"]["settings"], settings);

    settings["name"] = json!("Renamed");
    settings["size"] = json!("small");
    a.send_value(
        &json!({"type":"SetRoom", "request_id":"rename", "room_id":first["room_id"],
        "expected_revision":joined["revision"], "settings":settings, "capacity":4}),
    );
    for socket in [&mut a, &mut b] {
        loop {
            let updated = reply(socket, "RoomUpdated");
            if updated["lobby"]["settings"]["name"] == "Renamed" {
                assert_eq!(updated["lobby"]["settings"], settings);
                break;
            }
        }
    }
}

#[test]
fn hostile_settings_never_produce_malformed_and_never_affect_the_other_tab() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.hello_v2().unwrap();
    a.create_room_v2("create", "Ann");
    let ann = reply(&mut a, "Welcome");

    let mut b = Ws::open(s.http_addr());
    b.hello_v2().unwrap();
    b.send_value(
        &json!({"type":"JoinRoom", "request_id":"join", "room_id":ann["room_id"],
        "server_epoch":ann["server_epoch"], "name":"Bob", "settings":{
            "name":"N".repeat(60_000), "mode":[1], "size":{"x":null}, "minutes":1e300,
            "friendly_fire":[], "deep":[[[[[[[[1]]]]]]]]}}),
    );
    let bob = reply(&mut b, "Welcome");
    assert_eq!(bob["lobby"]["settings"], ann["lobby"]["settings"]);

    for (index, settings) in [
        json!(5),
        json!(null),
        json!({}),
        json!({"name":"x".repeat(40_000), "minutes":-1}),
    ]
    .into_iter()
    .enumerate()
    {
        b.send_value(
            &json!({"type":"SetRoom", "request_id":format!("hostile-{index}"),
            "room_id":ann["room_id"], "expected_revision":bob["revision"],
            "settings":settings, "capacity":4}),
        );
        assert_eq!(reply(&mut b, "Error")["code"], "NotOwner");
    }
    let directory = rooms(&mut a);
    assert_eq!(directory.len(), 1);
    assert_eq!(directory[0]["name"], ann["lobby"]["settings"]["name"]);
    assert_eq!(directory[0]["revision"], bob["revision"]);
    assert_eq!(directory[0]["connected_humans"], 2);
}

#[test]
fn hostile_creation_is_refused_without_admission_or_directory_mutation() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.hello_v2().unwrap();
    a.send_value(
        &json!({"type":"CreateRoom", "request_id":"invalid", "name":"Ann", "capacity":4,
        "settings":{"name":"\u{7}x".repeat(5_000), "minutes":77, "size":"huge"}}),
    );
    assert_eq!(reply(&mut a, "Error")["code"], "BadRoom");
    assert!(rooms(&mut a).is_empty());
    a.create_room_v2("after-rejection", "Ann");
    assert_eq!(reply(&mut a, "Welcome")["slot"], 0);
    assert_eq!(rooms(&mut a)[0]["connected_humans"], 1);
}
