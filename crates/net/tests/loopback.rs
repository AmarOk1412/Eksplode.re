mod common;

use std::time::Duration;

use bomberust_core::{Dir, Outcome};
use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_net::Client;
use common::*;

/// Everything a client saw of one match.
#[derive(Default)]
struct Record {
    start: Option<(u64, u32)>,
    first_arena: Option<bomberust_core::Arena>,
    last: Option<bomberust_core::Observation>,
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

fn quick_host() -> bomberust_net::Host {
    host_with(|c| c.game = quick_game(60))
}

#[test]
fn us3_as1_join_assigns_a_free_slot() {
    let host = host();
    let a = connect(host.addr(), "Ann");
    assert_eq!(welcome_slot(&a), 0);
    let b = connect(host.addr(), "Bob");
    assert_eq!(welcome_slot(&b), 1);
}

#[test]
fn us3_as2_everyone_sees_the_same_start() {
    let host = quick_host();
    let clients = join_all(&host, 3);
    host.add_bot();
    host.start_match();
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
    let clients = join_all(&host, 4);
    host.start_match();
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
    let c = connect(host.addr(), "Ann");
    welcome_slot(&c);
    host.add_bot();
    host.start_match();
    let r = follow(&c);
    assert!(r.ended.is_some());
    let lobby = wait_for(&c, |m| matches!(m, ServerMsg::Lobby { .. }));
    assert!(lobby.is_some());
    // and a rematch works
    host.start_match();
    assert!(wait_for(&c, |m| matches!(m, ServerMsg::Start { .. })).is_some());
}

#[test]
fn us3_as4_a_disconnect_is_replaced_by_a_bot_and_the_match_goes_on() {
    let host = host_with(|c| c.game = quick_game(100_000));
    let a = connect(host.addr(), "Ann");
    welcome_slot(&a);
    let b = connect(host.addr(), "Bob");
    welcome_slot(&b);
    host.start_match();
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
        if let ServerMsg::Snapshot { tick, .. } = m {
            last = tick;
        }
    }
    last
}

#[test]
fn us3_as5_malformed_and_out_of_turn_messages_change_nothing() {
    let host = host_with(|c| c.game = quick_game(100_000));
    let good = connect(host.addr(), "Ann");
    welcome_slot(&good);

    let mut bad = Raw::connect(host.addr());
    bad.send(&ClientMsg::Join {
        version: 1,
        name: "Mal".into(),
        look: None,
    });
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
    bad.send(&ClientMsg::Input {
        tick: 0,
        mv: Some(Dir::Up),
        bomb: true,
        punch: false,
    });
    let err = bad.recv_until(|m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(err, Some(ServerMsg::Error { code, .. }) if code == "WrongPhase"));

    host.start_match();
    // input from the future during the match
    bad.send(&ClientMsg::Input {
        tick: 1_000_000,
        mv: None,
        bomb: false,
        punch: false,
    });
    let err = bad.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "BadTick"));
    assert!(err.is_some());

    // the other player is unaffected and still gets snapshots
    assert!(wait_for(&good, |m| matches!(m, ServerMsg::Snapshot { .. })).is_some());
}

#[test]
fn ten_errors_close_the_connection() {
    let host = host();
    let mut bad = Raw::connect(host.addr());
    bad.send(&ClientMsg::Join {
        version: 1,
        name: "Mal".into(),
        look: None,
    });
    bad.recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .unwrap();
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
    let clients = join_all(&host, 4);
    host.start_match();
    let mut late = Raw::connect(host.addr());
    late.send(&ClientMsg::Join {
        version: 1,
        name: "late".into(),
        look: None,
    });
    assert!(matches!(late.recv(), Some(ServerMsg::Rejected { .. })));
    assert!(wait_for(&clients[0], |m| matches!(m, ServerMsg::Snapshot { .. })).is_some());
}

#[test]
fn a_silent_player_is_dropped_during_a_match() {
    let host = host_with(|c| {
        c.game = quick_game(100_000);
        c.silence_timeout = Duration::from_millis(300);
    });
    let me = host.local_client("Host").unwrap();
    welcome_slot(&me);
    let quiet = connect(host.addr(), "Quiet");
    welcome_slot(&quiet);
    host.start_match();
    // `me` keeps talking, `quiet` does not
    let mut left = None;
    for _ in 0..150 {
        let _ = me.send(ClientMsg::Ping { nonce: 1 });
        while let Ok(m) = me.try_recv() {
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
    let a = connect(host.addr(), "Ann");
    welcome_slot(&a);
    host.add_bot();
    host.start_match();
    wait_for(&a, |m| matches!(m, ServerMsg::Snapshot { .. })).unwrap();
    drop(host);
    let err = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Error { code, .. } if code == "HostLeft"),
    );
    assert!(err.is_some());
}

#[test]
fn inputs_from_a_player_move_their_character() {
    let host = host_with(|c| {
        c.game = bomberust_core::Config {
            block_density: 0.0,
            ..quick_game(100_000)
        };
    });
    let a = connect(host.addr(), "Ann");
    welcome_slot(&a);
    host.add_bot();
    host.start_match();
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
        }) = a.recv_timeout(Duration::from_millis(50))
        {
            tick = t;
            moved |= events.iter().any(
                |e| matches!(e, bomberust_core::Event::Moved { player, .. } if player.index() == 0),
            );
        }
        if moved {
            break;
        }
    }
    assert!(moved);
}
