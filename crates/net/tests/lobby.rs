#![allow(clippy::unwrap_used)]

mod common;

use common::wait_for_game as wait_for;
use common::*;
use eksplode_re_core::Controller;
use eksplode_re_net::protocol::v2::{Request, RoomRef};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::{Client, Host};

fn create(host: &Host, name: &str) -> (Client, RoomRef) {
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let room = client
        .create_room(name, &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert_eq!(welcome_slot(&client), 0);
    (client, room)
}

fn join(host: &Host, room: &RoomRef, name: &str) -> Client {
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    client.join_room(room, name).unwrap();
    client
}

#[test]
fn the_first_room_message_must_follow_hello() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send_value(
        &serde_json::json!({"type":"Start", "request_id":"before-hello", "room_id":"unknown"}),
    );
    assert_eq!(raw.recv_value().unwrap()["code"], "BadVersion");
    assert!(raw.recv_value().is_none(), "connection is closed");
}

#[test]
fn a_wrong_protocol_version_is_rejected() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send_value(&serde_json::json!({"type":"Hello", "version":3}));
    assert_eq!(
        raw.recv_value().unwrap(),
        serde_json::json!({"type":"Rejected", "reason":"BadVersion"})
    );
    assert!(raw.recv_value().is_none());
}

#[test]
fn bad_names_are_rejected() {
    let host = host();
    let (_owner, room) = create(&host, "Owner");
    for name in ["", "   ", "this name is far too long", "bad\u{7}name"] {
        let mut raw = Raw::connect(host.addr());
        assert!(raw.hello_v2().is_some());
        raw.join_room_v2("bad-name", &serde_json::to_value(&room).unwrap(), name);
        assert_eq!(raw.recv_value().unwrap()["code"], "BadName", "{name:?}");
    }
}

#[test]
fn joining_assigns_slots_in_order_and_welcomes() {
    let host = host();
    let (a, room) = create(&host, "Ann");
    let b = join(&host, &room, "Bob");
    assert_eq!(welcome_slot(&b), 1);
    // everyone sees the updated lobby
    let seen = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Remote),
    );
    assert!(seen.is_some());
}

#[test]
fn a_second_join_from_the_same_connection_is_an_error() {
    let host = host();
    let (_owner, room) = create(&host, "Owner");
    let mut raw = Raw::connect(host.addr());
    assert!(raw.hello_v2().is_some());
    raw.join_room_v2("first", &serde_json::to_value(&room).unwrap(), "Ann");
    assert!(raw
        .recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .is_some());
    raw.join_room_v2("second", &serde_json::to_value(&room).unwrap(), "Ann again");
    let err = raw.recv_until(|m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(err, Some(ServerMsg::Error { code, .. }) if code == "AlreadyInRoom"));
}

#[test]
fn the_fifth_join_is_refused_as_full() {
    let host = host();
    let (_clients, room) = join_room_all(&host, 4);
    let mut raw = Raw::connect(host.addr());
    assert!(raw.hello_v2().is_some());
    raw.join_room_v2("late", &serde_json::to_value(&room).unwrap(), "late");
    assert_eq!(raw.recv_value().unwrap()["code"], "Full");
}

#[test]
fn bots_fill_the_seats_and_humans_replace_them_up_to_the_limit() {
    // 011 FR-008: a new room starts full of bots; every joining human takes a bot seat
    let host = host();
    let (_owner, room) = create(&host, "Ann");
    assert!(
        host.handle().add_room_bot(&room).is_err(),
        "no empty seat is left"
    );
    let mut humans = Vec::new();
    for (name, seat) in [("Bob", 1), ("Cy", 2), ("Di", 3)] {
        let client = join(&host, &room, name);
        assert_eq!(welcome_slot(&client), seat);
        humans.push(client);
    }
    let mut raw = Raw::connect(host.addr());
    assert!(raw.hello_v2().is_some());
    raw.join_room_v2("late", &serde_json::to_value(&room).unwrap(), "late");
    assert_eq!(raw.recv_value().unwrap()["code"], "Full");
}

#[test]
fn joining_during_a_match_is_refused() {
    let host = host();
    let (a, room) = create(&host, "Ann");
    host.handle().start_room(&room).unwrap();
    assert!(wait_for(&a, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    let mut raw = Raw::connect(host.addr());
    assert!(raw.hello_v2().is_some());
    raw.join_room_v2("late", &serde_json::to_value(&room).unwrap(), "late");
    assert_eq!(raw.recv_value().unwrap()["code"], "InMatch");
}

#[test]
fn ready_flags_are_shared_and_leaving_gives_the_seat_to_a_bot() {
    let host = host();
    let (a, room) = create(&host, "Ann");
    let b = join(&host, &room, "Bob");
    welcome_slot(&b);
    b.send(ClientMsg::SetReady { ready: true }).unwrap();
    let seen = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].ready),
    );
    assert!(seen.is_some());

    b.send(ClientMsg::Leave).unwrap();
    // 011 FR-008: a seat left before the match is refilled with a bot
    let freed = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Bot),
    );
    assert!(freed.is_some());
}

#[test]
fn the_host_player_takes_a_local_slot() {
    let host = host();
    let me = host.connect(true).unwrap();
    me.send_v2(eksplode_re_net::protocol::v2::Request::Hello { version: 2 })
        .unwrap();
    let room = me
        .create_room("Host", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert_eq!(welcome_slot(&me), 0);
    let other = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    other.join_room(&room, "Ann").unwrap();
    assert_eq!(welcome_slot(&other), 1);
    let lobby = wait_for_game(
        &other,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[0].controller == Controller::Local),
    );
    assert!(lobby.is_some());
}

#[test]
fn a_local_connection_can_join_an_explicitly_selected_remote_room() {
    let host = host();
    let (owner, room) = create(&host, "Owner");
    let local = host.connect(true).unwrap();
    local.send_v2(Request::Hello { version: 2 }).unwrap();
    local.join_room(&room, "Local").unwrap();
    assert_eq!(welcome_slot(&local), 1);
    assert!(wait_for(&owner, |message| matches!(message,
        ServerMsg::Lobby { lobby, .. } if lobby.slots[0].controller == Controller::Remote
            && lobby.slots[1].controller == Controller::Local))
    .is_some());
}

#[test]
fn a_single_participant_can_start_a_match() {
    let host = host();
    let (owner, room) = create(&host, "Ann");
    host.handle().start_room(&room).unwrap();
    assert!(wait_for(&owner, |message| matches!(message, ServerMsg::Start { .. })).is_some());
}

#[test]
fn ping_gets_a_pong_even_before_joining() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&ClientMsg::Ping { nonce: 77 });
    assert_eq!(raw.recv(), Some(ServerMsg::Pong { nonce: 77 }));
}

fn error_code(client: &Client) -> Option<String> {
    match wait_for(client, |m| matches!(m, ServerMsg::Error { .. })) {
        Some(ServerMsg::Error { code, .. }) => Some(code),
        _ => None,
    }
}

#[test]
fn the_owner_can_start_and_add_or_remove_bots() {
    let host = host();
    let (owner, _room) = create(&host, "Ann");
    // without a seat, RemoveBot frees the highest bot and AddBot fills the first free seat
    owner.send(ClientMsg::RemoveBot).unwrap();
    let lobby = wait_for(
        &owner,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[3].controller == Controller::Empty),
    );
    assert!(lobby.is_some());
    owner.send(ClientMsg::AddBot).unwrap();
    let lobby = wait_for(
        &owner,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[3].controller == Controller::Bot),
    );
    assert!(lobby.is_some());
    owner.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&owner, |m| matches!(m, ServerMsg::Start { .. })).is_some());
}

#[test]
fn a_non_owner_gets_notowner_and_nothing_changes() {
    let host = host();
    let (owner, room) = create(&host, "Ann");
    let other = join(&host, &room, "Bob");
    welcome_slot(&other);
    for msg in [ClientMsg::Start, ClientMsg::RemoveBot] {
        other.send(msg.clone()).unwrap();
        assert_eq!(error_code(&other).as_deref(), Some("NotOwner"), "{msg:?}");
    }
    // 011 FR-010: adding a bot is open to everybody; here the seats are simply all taken
    other.send(ClientMsg::AddBot).unwrap();
    assert_eq!(error_code(&other).as_deref(), Some("Full"));
    // the owner never saw a Start
    owner.send(ClientMsg::Ping { nonce: 1 }).unwrap();
    let seen = wait_for(&owner, |m| {
        matches!(m, ServerMsg::Start { .. } | ServerMsg::Pong { .. })
    });
    assert!(matches!(seen, Some(ServerMsg::Pong { .. })));
}

#[test]
fn start_and_bot_commands_are_refused_during_a_match() {
    let host = host();
    let (owner, _room) = create(&host, "Ann");
    owner.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&owner, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    for command in [ClientMsg::Start, ClientMsg::AddBot, ClientMsg::RemoveBot] {
        owner.send(command).unwrap();
        assert_eq!(error_code(&owner).as_deref(), Some("InMatch"));
    }
}

#[test]
fn adding_a_bot_to_a_full_lobby_is_refused() {
    let host = host();
    let (owner, _room) = create(&host, "Ann");
    // the three other seats were filled with bots when the room was created
    owner.send(ClientMsg::AddBot).unwrap();
    assert_eq!(error_code(&owner).as_deref(), Some("Full"));
}

#[test]
fn ownership_moves_when_the_owner_leaves() {
    let host = host();
    let (first, room) = create(&host, "Ann");
    let second = join(&host, &room, "Bob");
    welcome_slot(&second);
    second.send(ClientMsg::Start).unwrap();
    assert_eq!(error_code(&second).as_deref(), Some("NotOwner"));
    first.send(ClientMsg::Leave).unwrap();
    // wait until the lobby shows slot 0 handed to a bot
    assert!(wait_for(&second, |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[0].controller == Controller::Bot))
        .is_some());
    second.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&second, |m| matches!(m, ServerMsg::Start { .. })).is_some());
}

#[test]
fn an_in_process_connection_has_not_joined_until_it_says_so() {
    let host = host();
    let c = host.connect(false).unwrap();
    c.send_v2(Request::Hello { version: 2 }).unwrap();
    c.send_v2(Request::Start {
        request_id: eksplode_re_net::protocol::v2::RequestId::new("unjoined-start").unwrap(),
        room_id: "unknown".into(),
    })
    .unwrap();
    let got = wait_for(&c, |m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(got, Some(ServerMsg::Error { code, .. }) if code == "NotInRoom"));

    c.create_room("Ann", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert_eq!(welcome_slot(&c), 0);
}
