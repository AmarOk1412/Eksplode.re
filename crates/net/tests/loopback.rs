mod common;

use std::time::Duration;

use common::wait_for_game as wait_for;
use common::*;
use eksplode_re_core::{Dir, Outcome};
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::Client;

/// Everything a client saw of one match.
#[derive(Default)]
struct Record {
    start: Option<(u64, u32)>,
    first_arena: Option<eksplode_re_core::Arena>,
    last: Option<eksplode_re_core::Observation>,
    ended: Option<Outcome>,
    snapshots: u32,
}

/// Plays idle inputs until the match ends (or the timeout hits) and records what was seen.
fn follow(c: &Client) -> Record {
    let mut r = Record::default();
    let mut tick = 0;
    while let Some(m) = wait_for(c, |_| true) {
        match m {
            ServerMsg::Start { seed, config } => r.start = Some((seed, config.round_limit_ticks)),
            ServerMsg::Snapshot { tick: t, state, .. } => {
                tick = t;
                r.snapshots += 1;
                r.first_arena.get_or_insert_with(|| state.arena.clone());
                r.last = Some(*state);
                let _ = c.send(ClientMsg::Input {
                    tick,
                    mv: None,
                    bomb: false,
                    punch: false,
                });
            }
            ServerMsg::Ended { outcome } => {
                r.ended = Some(outcome);
                break;
            }
            _ => {}
        }
    }
    let _ = tick;
    r
}

fn quick_host() -> eksplode_re_net::Host {
    host_with(|c| c.game = quick_game(60))
}

#[test]
fn us3_as1_join_assigns_a_free_slot() {
    let host = host();
    let a = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let room = a
        .create_room("Ann", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert_eq!(welcome_slot(&a), 0);
    let b = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    b.join_room(&room, "Bob").unwrap();
    assert_eq!(welcome_slot(&b), 1);
}

#[test]
fn us3_as2_everyone_sees_the_same_start() {
    let host = quick_host();
    let (clients, room) = join_room_all(&host, 3);
    // the seats nobody joined were filled with bots when the room was created (011)
    host.handle().start_room(&room).unwrap();
    let handles: Vec<_> = clients
        .into_iter()
        .map(|c| std::thread::spawn(move || follow(&c)))
        .collect();
    let records: Vec<Record> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for r in &records {
        assert_eq!(r.start, Some((11, 60)));
        assert_eq!(
            r.first_arena, records[0].first_arena,
            "same arena for everyone"
        );
    }
}

#[test]
fn us3_as3_all_clients_agree_on_state_and_outcome() {
    let host = quick_host();
    let (clients, room) = join_room_all(&host, 4);
    host.handle().start_room(&room).unwrap();
    let handles: Vec<_> = clients
        .into_iter()
        .map(|c| std::thread::spawn(move || follow(&c)))
        .collect();
    let records: Vec<Record> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let outcome = records[0].ended.expect("round ended");
    for (i, r) in records.iter().enumerate() {
        assert_eq!(r.ended, Some(outcome), "client {i}");
        let (a, b) = (r.last.as_ref().unwrap(), records[0].last.as_ref().unwrap());
        assert_eq!(
            (
                a.tick,
                &a.arena,
                &a.players,
                &a.bombs,
                &a.flames,
                &a.powerups,
                a.phase
            ),
            (
                b.tick,
                &b.arena,
                &b.players,
                &b.bombs,
                &b.flames,
                &b.powerups,
                b.phase
            ),
            "client {i} saw a different final state"
        );
        assert_eq!(a.me.index(), i, "each client gets its own view");
    }
    assert_eq!(records[0].last.as_ref().unwrap().tick, 60);
}

#[test]
fn the_host_returns_to_the_lobby_after_the_round() {
    let host = quick_host();
    let (mut clients, room) = join_room_all(&host, 1);
    let c = clients.pop().unwrap();
    // the seats nobody joined were filled with bots when the room was created (011)
    host.handle().start_room(&room).unwrap();
    let r = follow(&c);
    assert!(r.ended.is_some());
    let lobby = wait_for(&c, |m| matches!(m, ServerMsg::Lobby { .. }));
    assert!(lobby.is_some());
    // and a rematch works
    host.handle().start_room(&room).unwrap();
    assert!(wait_for(&c, |m| matches!(m, ServerMsg::Start { .. })).is_some());
}

#[test]
fn us3_as4_a_disconnect_is_replaced_by_a_bot_and_the_match_goes_on() {
    let host = host_with(|c| c.game = quick_game(100_000));
    let (mut clients, room) = join_room_all(&host, 2);
    let b = clients.pop().unwrap();
    let a = clients.pop().unwrap();
    host.handle().start_room(&room).unwrap();
    wait_for(&a, |m| matches!(m, ServerMsg::Snapshot { .. })).expect("snapshots flow");
    drop(b);
    let left = wait_for(&a, |m| matches!(m, ServerMsg::PlayerLeft { .. }));
    assert_eq!(
        left,
        Some(ServerMsg::PlayerLeft {
            slot: 1,
            replaced_by_bot: true
        })
    );
    // the match keeps producing ticks, and slot 1 is now a bot
    std::thread::sleep(Duration::from_millis(50));
    let t1 = latest_tick(&a);
    std::thread::sleep(Duration::from_millis(100));
    let t2 = latest_tick(&a);
    assert!(t2 > t1, "ticks keep coming ({t1} -> {t2})");
}

/// Tick of the newest snapshot waiting in the channel (drains it).
fn latest_tick(c: &Client) -> u32 {
    let mut last = 0;
    while let Ok(m) = c.try_recv() {
        if let ServerMsg::Snapshot { tick, .. } = m.into_game_message() {
            last = tick;
        }
    }
    last
}

#[test]
fn us3_as5_malformed_and_out_of_turn_messages_change_nothing() {
    let host = host_with(|c| c.game = quick_game(100_000));
    let (mut clients, room) = join_room_all(&host, 1);
    let good = clients.pop().unwrap();

    let mut bad = Raw::connect(host.addr());
    assert!(bad.hello_v2().is_some());
    bad.join_room_v2("mal-join", &serde_json::to_value(&room).unwrap(), "Mal");
    bad.recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .unwrap();
    for line in [
        "garbage",
        "{\"type\":\"Nope\"}",
        "{\"type\":\"Input\",\"tick\":\"x\"}",
    ] {
        bad.send_line(line);
        let err = bad.recv_until(|m| matches!(m, ServerMsg::Error { .. }));
        assert!(matches!(err, Some(ServerMsg::Error { code, .. }) if code == "Malformed"));
    }
    // input while no match runs
    bad.send_value(&serde_json::json!({
        "type": "Input", "room_id": room.room_id, "tick": 0,
        "mv": Dir::Up, "bomb": true, "punch": false
    }));
    let err = bad.recv_until(|m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(err, Some(ServerMsg::Error { code, .. }) if code == "InMatch"));

    host.handle().start_room(&room).unwrap();
    // input from the future during the match
    bad.send_value(&serde_json::json!({
        "type": "Input", "room_id": room.room_id, "tick": 1_000_000,
        "mv": null, "bomb": false, "punch": false
    }));
    let err = bad.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "BadRoom"));
    assert!(err.is_some());

    // the other player is unaffected and still gets snapshots
    assert!(wait_for(&good, |m| matches!(m, ServerMsg::Snapshot { .. })).is_some());
}

#[test]
fn ten_errors_close_the_connection() {
    let host = host();
    let mut bad = Raw::connect(host.addr());
    assert!(bad.hello_v2().is_some());
    for _ in 0..10 {
        bad.send_line("garbage");
    }
    let mut errors = 0;
    while let Some(m) = bad.recv() {
        if matches!(m, ServerMsg::Error { .. }) {
            errors += 1;
        }
    }
    assert_eq!(errors, 10, "closed right after the tenth error");
}

#[test]
fn an_oversized_line_closes_the_connection() {
    let host = host();
    let mut bad = Raw::connect(host.addr());
    bad.send_line(&"a".repeat(70_000));
    assert!(bad
        .recv_until(|m| matches!(m, ServerMsg::Error { .. }))
        .is_some());
    assert!(bad.recv().is_none());
}

#[test]
fn us3_as6_a_fifth_player_is_refused_while_the_others_play_on() {
    let host = host_with(|c| c.game = quick_game(100_000));
    let (clients, room) = join_room_all(&host, 4);
    host.handle().start_room(&room).unwrap();
    let mut late = Raw::connect(host.addr());
    assert!(late.hello_v2().is_some());
    late.join_room_v2("late", &serde_json::to_value(room).unwrap(), "late");
    assert_eq!(late.recv_value().unwrap()["code"], "InMatch");
    assert!(wait_for(&clients[0], |m| matches!(m, ServerMsg::Snapshot { .. })).is_some());
}

#[test]
fn a_silent_player_is_dropped_during_a_match() {
    let host = host_with(|c| {
        c.game = quick_game(100_000);
        c.silence_timeout = Duration::from_millis(300);
    });
    let (clients, room) = join_room_all(&host, 2);
    let me = &clients[0];
    host.handle().start_room(&room).unwrap();
    // `me` keeps talking, `quiet` does not
    let mut left = None;
    for _ in 0..150 {
        let _ = me.send(ClientMsg::Ping { nonce: 1 });
        while let Ok(m) = me.try_recv() {
            let m = m.into_game_message();
            if matches!(m, ServerMsg::PlayerLeft { .. }) {
                left = Some(m);
            }
        }
        if left.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        left,
        Some(ServerMsg::PlayerLeft {
            slot: 1,
            replaced_by_bot: true
        })
    );
}

#[test]
fn the_host_leaving_tells_everyone() {
    let host = host();
    let (mut clients, room) = join_room_all(&host, 1);
    let a = clients.pop().unwrap();
    // the seats nobody joined were filled with bots when the room was created (011)
    host.handle().start_room(&room).unwrap();
    wait_for(&a, |m| matches!(m, ServerMsg::Snapshot { .. })).unwrap();
    drop(host);
    let err = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Error { code, .. } if code == "HostLeft"),
    );
    assert!(err.is_some());
}

#[test]
fn the_host_leaving_tells_unjoined_directory_clients() {
    let host = host();
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    assert!(wait_for(&client, |message| {
        matches!(message, ServerMsg::RoomV2(reply)
            if matches!(reply.as_ref(), eksplode_re_net::protocol::v2::Reply::Hello { .. }))
    })
    .is_some());
    drop(host);
    assert!(wait_for(&client, |message| {
        matches!(message, ServerMsg::Error { code, .. } if code == "Unavailable")
    })
    .is_some());
}

#[test]
fn inputs_from_a_player_move_their_character() {
    let host = host_with(|c| {
        c.game = eksplode_re_core::Config {
            block_density: 0.0,
            ..quick_game(100_000)
        };
    });
    let (clients, room) = join_room_all(&host, 1);
    let a = &clients[0];
    // the seats nobody joined were filled with bots when the room was created (011)
    host.handle().start_room(&room).unwrap();
    let mut moved = false;
    let mut tick = 0;
    for _ in 0..200 {
        let _ = a.send(ClientMsg::Input {
            tick,
            mv: Some(Dir::Down),
            bomb: false,
            punch: false,
        });
        if let Ok(ServerMsg::Snapshot {
            tick: t, events, ..
        }) = a
            .recv_timeout(Duration::from_millis(50))
            .map(ServerMsg::into_game_message)
        {
            tick = t;
            moved |= events.iter().any(
                |e| matches!(e, eksplode_re_core::Event::Moved { player, .. } if player.index() == 0),
            );
        }
        if moved {
            break;
        }
    }
    assert!(moved);
}
