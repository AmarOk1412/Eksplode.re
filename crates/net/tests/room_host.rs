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

#[test]
fn f4_us1_as6_the_match_uses_the_room_size_and_time() {
    let sizes = [("small", 15u8, 13u8), ("medium", 19, 15), ("large", 25, 19)];
    for (size, w, h) in sizes {
        for minutes in [2u32, 3, 5, 10] {
            let host = host();
            let room = json!({"size": size, "minutes": minutes});
            let (mut a, _, _) = join(&host, "Ann", Some(room));
            a.send(&ClientMsg::Start);
            let config = match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
                Some(ServerMsg::Start { config, .. }) => config,
                other => panic!("{other:?}"),
            };
            assert_eq!((config.width, config.height), (w, h), "{size}");
            assert_eq!(
                config.round_limit_ticks,
                minutes * 60 * 20,
                "{size} {minutes}"
            );
            // the pattern always starts 60 seconds before the end
            assert_eq!(
                config.round_limit_ticks - config.closing_ticks,
                (minutes - 1) * 60 * 20
            );
            // the first snapshot carries the same configuration and the arena of that size
            match a.recv_until(|m| matches!(m, ServerMsg::Snapshot { .. })) {
                Some(ServerMsg::Snapshot { state, .. }) => {
                    assert_eq!((state.arena.width(), state.arena.height()), (w, h));
                    assert_eq!(state.config.round_limit_ticks, config.round_limit_ticks);
                }
                other => panic!("{other:?}"),
            }
        }
    }
}

#[test]
fn a_change_by_the_owner_before_the_start_is_used() {
    let host = host();
    let (mut a, _, _) = join(&host, "Ann", None);
    a.send_line(r#"{"type":"SetRoom","settings":{"size":"medium","minutes":5}}"#);
    lobby_settings(&mut a, |s| s.minutes == 5);
    a.send_line(r#"{"type":"SetRoom","settings":{"size":"large","minutes":2}}"#);
    lobby_settings(&mut a, |s| s.minutes == 2);
    a.send(&ClientMsg::Start);
    match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
        Some(ServerMsg::Start { config, .. }) => {
            assert_eq!(
                (config.width, config.height, config.round_limit_ticks),
                (25, 19, 2400)
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_team_room_is_accepted_and_the_other_modes_follow_their_rules() {
    let host = host();
    let (mut a, _, lobby) = join(&host, "Ann", Some(json!({"mode": "team"})));
    assert_eq!(lobby.settings.mode, Mode::Team);
    a.send_line(r#"{"type":"SetRoom","settings":{"mode":"team","friendly_fire":false}}"#);
    let s = lobby_settings(&mut a, |s| !s.friendly_fire);
    assert_eq!(s.mode, Mode::Team);
    assert!(!s.friendly_fire);
}

#[test]
fn f4_us1_as5_a_start_with_too_few_participants_is_refused_with_the_missing_count() {
    let host = host();
    let (mut a, _, _) = join(
        &host,
        "Ann",
        Some(json!({"mode": "team", "friendly_fire": false})),
    );
    for (missing, add_bot) in [(3, true), (2, true), (1, false)] {
        a.send(&ClientMsg::Start);
        let message = match a.recv_until(is_error("CannotStart")) {
            Some(ServerMsg::Error { message, .. }) => message,
            other => panic!("{other:?}"),
        };
        assert!(message.contains(&format!("{missing} missing")), "{message}");
        assert!(
            message.contains("bots"),
            "the message says bots can fill the slots: {message}"
        );
        if add_bot {
            a.send(&ClientMsg::AddBot);
            a.recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() > 4 - missing as usize))
                .unwrap();
        }
    }
    // a second human and two bots make four
    let (_b, _, _) = join(&host, "Bob", None);
    a.send(&ClientMsg::AddBot);
    a.send(&ClientMsg::AddBot);
    a.recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4))
        .unwrap();
    a.send(&ClientMsg::Start);
    match a.recv_until(|m| matches!(m, ServerMsg::Start { .. })) {
        Some(ServerMsg::Start { config, .. }) => {
            assert_eq!(config.mode, Mode::Team);
            assert!(!config.friendly_fire);
        }
        other => panic!("{other:?}"),
    }
    // teams reach the observation
    match a.recv_until(|m| matches!(m, ServerMsg::Snapshot { .. })) {
        Some(ServerMsg::Snapshot { state, .. }) => {
            let teams: Vec<_> = state.players.iter().map(|p| p.team).collect();
            assert_eq!(teams, vec![Some(0), Some(0), Some(1), Some(1)]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f4_us4_dev_commands_still_work_in_a_chaser_room() {
    let host = host_with(|c| c.dev = true);
    let (mut a, _, lobby) = join(&host, "Ann", Some(json!({"mode": "chaser"})));
    assert_eq!(lobby.settings.mode, Mode::Chaser);
    a.send_line(r#"{"type":"DevPauseBots","paused":true}"#);
    a.send_line(r#"{"type":"DevSetDrops","chances":[0.5,0,0,0,0,0,0,0]}"#);
    let state = a.recv_until(|m| matches!(m, ServerMsg::DevState { drops, .. } if drops[0] == 0.5));
    match state {
        Some(ServerMsg::DevState { paused, drops }) => {
            assert!(paused);
            assert_eq!(drops[0], 0.5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_chaser_room_starts_with_four_participants_and_one_chaser() {
    let host = host();
    let (mut a, _, _) = join(&host, "Ann", Some(json!({"mode": "chaser"})));
    a.send(&ClientMsg::Start);
    match a.recv_until(is_error("CannotStart")) {
        Some(ServerMsg::Error { message, .. }) => assert!(
            message.contains("Chaser") && message.contains("3 missing"),
            "{message}"
        ),
        other => panic!("{other:?}"),
    }
    for _ in 0..3 {
        a.send(&ClientMsg::AddBot);
    }
    a.recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4))
        .unwrap();
    a.send(&ClientMsg::Start);
    match a.recv_until(|m| matches!(m, ServerMsg::Snapshot { .. })) {
        Some(ServerMsg::Snapshot { state, .. }) => {
            use bomberust_core::Role;
            assert_eq!(
                state
                    .players
                    .iter()
                    .filter(|p| p.role == Role::Chaser)
                    .count(),
                1
            );
            assert_eq!(
                state
                    .players
                    .iter()
                    .filter(|p| p.role == Role::Runner)
                    .count(),
                3
            );
        }
        other => panic!("{other:?}"),
    }
}
