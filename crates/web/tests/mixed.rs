//! Matches that mix browser (WebSocket) players, terminal (TCP) players and bots.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::Shutdown;
use std::time::Duration;

use bomberust_core::{Observation, Outcome};
use bomberust_net::protocol::{ClientMsg, RejectReason, ServerMsg};
use bomberust_net::Client;
use bomberust_web::Server;
use common::*;

fn tcp(s: &Server, name: &str) -> (Client, u8) {
    let c = Client::connect(&s.game_addr().to_string(), name).unwrap();
    let slot = match tcp_wait(&c, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, .. }) => slot,
        other => panic!("expected Welcome, got {other:?}"),
    };
    (c, slot)
}

fn tcp_wait(c: &Client, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
    while let Ok(m) = c.recv_timeout(WAIT) {
        if pred(&m) {
            return Some(m);
        }
    }
    None
}

fn is_snapshot(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::Snapshot { .. })
}

/// A web owner (slot 0), a terminal player (slot 1) and one bot.
fn mixed(s: &Server) -> (Ws, Client) {
    let mut ws = Ws::open(s.http_addr());
    ws.join("Web");
    assert_eq!(ws.welcome_slot(), 0);
    let (term, slot) = tcp(s, "Term");
    assert_eq!(slot, 1);
    ws.send(&ClientMsg::AddBot);
    (ws, term)
}

fn without_me(mut o: Observation) -> Observation {
    o.me = bomberust_core::PlayerId::new(0).unwrap();
    o
}

fn short_match(seed: u64) -> Server {
    server(|o| {
        o.seed = Some(seed);
        o.game.block_density = 0.3;
        o.game.round_limit_ticks = 60;
    })
}

#[test]
fn us2_as1_same_start() {
    let s = server(|o| o.game.block_density = 0.3);
    let (mut ws, term) = mixed(&s);
    ws.send(&ClientMsg::Start);

    let web_start = ws.recv_until(|m| matches!(m, ServerMsg::Start { .. }));
    let term_start = tcp_wait(&term, |m| matches!(m, ServerMsg::Start { .. }));
    let (Some(ServerMsg::Start { seed: a, .. }), Some(ServerMsg::Start { seed: b, .. })) =
        (web_start, term_start)
    else {
        panic!("both must receive Start");
    };
    assert_eq!(a, b);

    let web_first = ws.recv_until(is_snapshot);
    let term_first = tcp_wait(&term, is_snapshot);
    let (Some(ServerMsg::Snapshot { state: w, .. }), Some(ServerMsg::Snapshot { state: t, .. })) =
        (web_first, term_first)
    else {
        panic!("both must receive a snapshot");
    };
    assert_eq!(w.arena, t.arena, "identical first arena");
    assert_eq!(w.config, t.config);
    assert_eq!(w.me.index(), 0);
    assert_eq!(t.me.index(), 1);
}

/// Reads a web client to the end of the round: the last snapshot and the outcome.
fn web_result(ws: &mut Ws) -> (Observation, Outcome) {
    let mut last = None;
    while let Some(m) = ws.recv() {
        match m {
            ServerMsg::Snapshot { state, .. } => last = Some(*state),
            ServerMsg::Ended { outcome } => return (last.expect("a snapshot"), outcome),
            _ => {}
        }
    }
    panic!("the match never ended");
}

fn term_result(c: &Client) -> (Observation, Outcome) {
    let mut last = None;
    while let Ok(m) = c.recv_timeout(WAIT) {
        match m {
            ServerMsg::Snapshot { state, .. } => last = Some(*state),
            ServerMsg::Ended { outcome } => return (last.expect("a snapshot"), outcome),
            _ => {}
        }
    }
    panic!("the match never ended");
}

#[test]
fn us2_as2_same_result() {
    for round in 0..5 {
        let s = short_match(10 + round);
        let (mut ws, term) = mixed(&s);
        ws.send(&ClientMsg::Start);
        let (web_state, web_outcome) = web_result(&mut ws);
        let (term_state, term_outcome) = term_result(&term);
        assert_eq!(web_outcome, term_outcome, "round {round}");
        assert_eq!(web_state.tick, term_state.tick, "round {round}");
        assert_eq!(
            without_me(web_state),
            without_me(term_state),
            "round {round}"
        );
    }
}

/// Waits for the takeover of `slot` and then for two further ticks.
fn expect_takeover(term: &Client, slot: u8) {
    let left = tcp_wait(term, |m| matches!(m, ServerMsg::PlayerLeft { .. }));
    assert_eq!(
        left,
        Some(ServerMsg::PlayerLeft {
            slot,
            replaced_by_bot: true
        })
    );
    let mut ticks = Vec::new();
    while ticks.len() < 2 {
        match tcp_wait(term, is_snapshot) {
            Some(ServerMsg::Snapshot { tick, .. }) => ticks.push(tick),
            _ => panic!("ticks stopped after the takeover"),
        }
    }
    assert!(ticks[1] > ticks[0]);
}

fn started_mixed() -> (Server, Ws, Client) {
    let s = server(|_| {});
    let (mut ws, term) = mixed(&s);
    ws.send(&ClientMsg::Start);
    assert!(ws.recv_until(is_snapshot).is_some());
    assert!(tcp_wait(&term, is_snapshot).is_some());
    (s, ws, term)
}

#[test]
fn us2_as3_closing_the_page_gives_a_bot_takeover() {
    for _ in 0..10 {
        let (_s, mut ws, term) = started_mixed();
        let _ = ws.socket.close(None);
        let _ = ws.socket.flush();
        drop(ws);
        expect_takeover(&term, 0);
    }
}

#[test]
fn a_reset_browser_connection_behaves_the_same() {
    for _ in 0..10 {
        let (_s, mut ws, term) = started_mixed();
        ws.socket.get_mut().shutdown(Shutdown::Both).unwrap();
        drop(ws);
        expect_takeover(&term, 0);
    }
}

#[test]
fn a_silent_browser_is_replaced_after_the_silence_timeout() {
    let s = server(|o| o.silence_timeout = Duration::from_millis(600));
    let (term, slot) = tcp(&s, "Term");
    assert_eq!(slot, 0);
    let mut ws = Ws::open(s.http_addr());
    ws.join("Quiet");
    assert_eq!(ws.welcome_slot(), 1);
    term.send(ClientMsg::Start).unwrap();
    let mut tick = 0;
    let mut left = false;
    while let Ok(m) = term.recv_timeout(WAIT) {
        match m {
            ServerMsg::Snapshot { tick: t, .. } => {
                tick = t;
                // keep the terminal player alive
                term.send(ClientMsg::Input {
                    tick,
                    mv: None,
                    bomb: false,
                    punch: false,
                })
                .unwrap();
            }
            ServerMsg::PlayerLeft {
                slot: 1,
                replaced_by_bot: true,
            } => {
                left = true;
                break;
            }
            _ => {}
        }
    }
    assert!(left, "the silent page was not replaced (last tick {tick})");
}

#[test]
fn us2_as4_a_fifth_participant_is_refused() {
    let s = server(|_| {});
    let mut a = Ws::open(s.http_addr());
    a.join("Ann");
    assert_eq!(a.welcome_slot(), 0);
    let (_t1, slot) = tcp(&s, "Ann");
    assert_eq!(slot, 1);
    // two tabs from the same address are two players, and the same name is fine
    let mut b = Ws::open(s.http_addr());
    b.join("Ann");
    assert_eq!(b.welcome_slot(), 2);
    let (_t2, slot) = tcp(&s, "Bob");
    assert_eq!(slot, 3);

    let lobby =
        a.recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.participants() == 4));
    match lobby {
        Some(ServerMsg::Lobby { lobby, .. }) => {
            let names: Vec<&str> = lobby.slots.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, ["Ann", "Ann", "Ann", "Bob"]);
        }
        other => panic!("no full lobby: {other:?}"),
    }

    let mut late = Ws::open(s.http_addr());
    late.join("Late");
    assert_eq!(
        late.recv(),
        Some(ServerMsg::Rejected {
            reason: RejectReason::Full
        })
    );
    let late_tcp = Client::connect(&s.game_addr().to_string(), "Late").unwrap();
    assert_eq!(
        late_tcp.recv_timeout(WAIT),
        Ok(ServerMsg::Rejected {
            reason: RejectReason::Full
        })
    );
}

#[test]
fn us3_as3_cli_join_sees_browser_player() {
    let s = server(|_| {});
    let mut ws = Ws::open(s.http_addr());
    ws.join("Webby");
    ws.welcome_slot();
    let c = Client::connect(&s.game_addr().to_string(), "Termy").unwrap();
    match tcp_wait(&c, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, lobby, .. }) => {
            assert_eq!(slot, 1);
            assert_eq!(lobby.slots[0].name, "Webby");
        }
        other => panic!("{other:?}"),
    }
}

/// A browser or a terminal participant seen as a message channel.
enum Endpoint {
    Web(Box<Ws>),
    Term(Client),
}

impl Endpoint {
    fn send(&mut self, msg: ClientMsg) {
        match self {
            Endpoint::Web(ws) => ws.send(&msg),
            Endpoint::Term(c) => {
                let _ = c.send(msg);
            }
        }
    }

    fn recv(&mut self) -> Option<ServerMsg> {
        match self {
            Endpoint::Web(ws) => ws.recv(),
            Endpoint::Term(c) => c.recv_timeout(WAIT).ok(),
        }
    }
}

/// Walks a column and back, timing how long each step takes to show up in a snapshot.
fn drive(mut end: Endpoint, slot: usize) -> Vec<Duration> {
    use bomberust_core::{Dir, Event};
    use std::time::Instant;

    let (first, back) = if slot < 2 {
        (Dir::Down, Dir::Up)
    } else {
        (Dir::Up, Dir::Down)
    };
    let dirs: Vec<Dir> = [first; 5].into_iter().chain([back; 5]).collect();
    let (mut latencies, mut idx, mut sent_at) = (Vec::new(), 0, None::<Instant>);
    while let Some(m) = end.recv() {
        match m {
            ServerMsg::Snapshot { tick, events, .. } => {
                let moved = events
                    .iter()
                    .any(|e| matches!(e, Event::Moved { player, .. } if player.index() == slot));
                if let (true, Some(t0)) = (moved, sent_at) {
                    latencies.push(t0.elapsed());
                    sent_at = None;
                    idx += 1;
                }
                let mv = dirs.get(idx).copied();
                if mv.is_none() {
                    return latencies;
                }
                if sent_at.is_none() {
                    sent_at = Some(Instant::now());
                }
                end.send(ClientMsg::Input {
                    tick,
                    mv,
                    bomb: false,
                    punch: false,
                });
            }
            ServerMsg::Ended { .. } => break,
            _ => {}
        }
    }
    latencies
}

#[test]
fn sc003_key_to_snapshot_latency_with_two_browsers_and_two_terminals() {
    // the real 20 Hz tick; a one-tick move cooldown so a step does not wait for the cooldown
    let s = server(|o| {
        o.tick = Duration::from_millis(50);
        o.game.block_density = 0.0;
        o.game.base_move_cooldown = 1;
        o.game.min_move_cooldown = 1;
    });
    let mut ws0 = Ws::open(s.http_addr());
    ws0.join("w0");
    assert_eq!(ws0.welcome_slot(), 0);
    let (t1, slot) = tcp(&s, "t1");
    assert_eq!(slot, 1);
    let mut ws2 = Ws::open(s.http_addr());
    ws2.join("w2");
    assert_eq!(ws2.welcome_slot(), 2);
    let (t3, slot) = tcp(&s, "t3");
    assert_eq!(slot, 3);

    ws0.send(&ClientMsg::Start);
    let endpoints = vec![
        Endpoint::Web(Box::new(ws0)),
        Endpoint::Term(t1),
        Endpoint::Web(Box::new(ws2)),
        Endpoint::Term(t3),
    ];
    let handles: Vec<_> = endpoints
        .into_iter()
        .enumerate()
        .map(|(slot, e)| std::thread::spawn(move || drive(e, slot)))
        .collect();
    let mut all: Vec<Duration> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert!(all.len() >= 30, "only {} moves were observed", all.len());
    all.sort();
    let p95 = all[(all.len() * 95 / 100).min(all.len() - 1)];
    println!("{} samples, p95 latency {:?}", all.len(), p95);
    assert!(p95 < Duration::from_millis(200), "p95 latency {p95:?}");
}
