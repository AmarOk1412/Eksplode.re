//! Matches that mix browser (WebSocket) players, terminal (TCP) players and bots.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::Shutdown;
use std::time::Duration;

use common::*;
use eksplode_re_core::{Observation, Outcome};
use eksplode_re_net::protocol::v2::RoomRef;
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::Client;
use eksplode_re_web::Server;

fn tcp(s: &Server, room: &RoomRef, name: &str) -> (Client, u8) {
    let c = Client::connect_unjoined(&s.game_addr().to_string()).unwrap();
    c.join_room(room, name).unwrap();
    let slot = match tcp_game(&c, |m| matches!(m, ServerMsg::Welcome { .. })) {
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

fn tcp_game(c: &Client, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
    tcp_wait(c, |message| pred(&message.clone().into_game_message()))
        .map(ServerMsg::into_game_message)
}

fn v2_ws_reply(socket: &mut Ws, kind: &str) -> serde_json::Value {
    let deadline = std::time::Instant::now() + WAIT;
    while std::time::Instant::now() < deadline {
        let reply = socket.recv_value().expect("v2 reply before timeout");
        if reply["type"] == kind {
            return reply;
        }
    }
    panic!("expected {kind}");
}

#[test]
fn v2_mixed_clients_share_only_their_selected_room_game() {
    use eksplode_re_net::protocol::v2::{Reply, Request, RequestId};
    use serde_json::json;
    let server = server(|options| options.tick = Duration::from_millis(20));
    let mut first = Ws::open(server.http_addr());
    first.hello_v2().unwrap();
    first.create_room_v2("create", "First");
    let welcome = v2_ws_reply(&mut first, "Welcome");
    let mut second = Ws::open(server.http_addr());
    second.hello_v2().unwrap();
    second.create_room_v2("create", "Second");
    let other_room = v2_ws_reply(&mut second, "Welcome");
    let terminal = Client::connect_unjoined(&server.game_addr().to_string()).unwrap();
    terminal.recv_timeout(WAIT).unwrap();
    terminal
        .send_v2(Request::JoinRoom {
            request_id: RequestId::new("join").unwrap(),
            server_epoch: welcome["server_epoch"].as_str().unwrap().into(),
            room_id: welcome["room_id"].as_str().unwrap().into(),
            name: "Terminal".into(),
            password: None,
            look: None,
        })
        .unwrap();
    assert!(matches!(
        tcp_wait(
            &terminal,
            |message| matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), Reply::Welcome { .. }))
        ),
        Some(ServerMsg::RoomV2(_))
    ));
    for index in 0..2 {
        first.send_value(&json!({"type":"AddBot","request_id":format!("bot-{index}"),"room_id":welcome["room_id"]}));
    }
    first.send_value(&json!({"type":"Start","request_id":"start","room_id":welcome["room_id"]}));
    v2_ws_reply(&mut first, "Start");
    let web_snapshot = v2_ws_reply(&mut first, "Snapshot");
    let tcp_snapshot = tcp_wait(&terminal, |message| {
        matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), Reply::Snapshot { tick: 0, .. }))
    })
    .unwrap();
    let web_reply: Reply = serde_json::from_value(web_snapshot).unwrap();
    match (web_reply, tcp_snapshot) {
        (
            Reply::Snapshot {
                state: web,
                reference,
                ..
            },
            ServerMsg::RoomV2(tcp),
        ) => match *tcp {
            Reply::Snapshot {
                state,
                reference: tcp_reference,
                ..
            } => {
                assert_eq!(reference, tcp_reference);
                assert_eq!(without_me(*web), without_me(*state));
            }
            _ => panic!("expected TCP snapshot"),
        },
        _ => panic!("expected room-scoped snapshots"),
    }
    second.send_value(&json!({"type":"ListRooms","request_id":"list","scope":"local"}));
    let directory = v2_ws_reply(&mut second, "Rooms");
    let rooms = directory["rooms"].as_array().unwrap();
    assert_eq!(
        rooms
            .iter()
            .find(|room| room["room_id"] == welcome["room_id"])
            .unwrap()["phase"],
        "running"
    );
    assert_eq!(
        rooms
            .iter()
            .find(|room| room["room_id"] == other_room["room_id"])
            .unwrap()["phase"],
        "lobby"
    );
}

#[test]
fn v2_round_result_is_shared_only_by_its_room_members() {
    use eksplode_re_net::protocol::v2::{Reply, Request, RequestId};
    use serde_json::json;
    let server = server(|options| {
        options.game.round_limit_ticks = 40;
        options.game.block_density = 0.0;
    });
    let mut first = Ws::open(server.http_addr());
    first.hello_v2().unwrap();
    first.create_room_v2("create", "Playing");
    let room = v2_ws_reply(&mut first, "Welcome");
    let mut other = Ws::open(server.http_addr());
    other.hello_v2().unwrap();
    other.create_room_v2("create", "Waiting");
    let waiting = v2_ws_reply(&mut other, "Welcome");
    let terminal = Client::connect_unjoined(&server.game_addr().to_string()).unwrap();
    terminal.recv_timeout(WAIT).unwrap();
    terminal
        .send_v2(Request::JoinRoom {
            request_id: RequestId::new("join").unwrap(),
            server_epoch: room["server_epoch"].as_str().unwrap().into(),
            room_id: room["room_id"].as_str().unwrap().into(),
            name: "Terminal".into(),
            password: None,
            look: None,
        })
        .unwrap();
    tcp_wait(&terminal, |message| matches!(message, ServerMsg::RoomV2(reply) if matches!(reply.as_ref(), Reply::Welcome { .. }))).unwrap();
    first.send_value(&json!({"type":"Start","request_id":"start","room_id":room["room_id"]}));
    let web_end = v2_ws_reply(&mut first, "Ended");
    let terminal_end = tcp_wait(&terminal, |message| {
        matches!(message, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), Reply::Ended { .. }))
    })
    .unwrap();
    match terminal_end {
        ServerMsg::RoomV2(reply) => match *reply {
            Reply::Ended { reference, outcome } => {
                assert_eq!(reference.room_id, room["room_id"].as_str().unwrap());
                assert_eq!(serde_json::to_value(outcome).unwrap(), web_end["outcome"]);
            }
            _ => panic!("expected terminal result"),
        },
        _ => panic!("expected scoped result"),
    }
    other.send_value(&json!({"type":"ListRooms","request_id":"list","scope":"local"}));
    let directory = v2_ws_reply(&mut other, "Rooms");
    assert_eq!(
        directory["rooms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|listed| listed["room_id"] == waiting["room_id"])
            .unwrap()["phase"],
        "lobby"
    );
}

/// A web owner (slot 0), a terminal player (slot 1) and one bot.
fn mixed(s: &Server) -> (RoomWs, Client) {
    let mut ws = RoomWs::create(s.http_addr(), "Web");
    assert_eq!(ws.welcome_slot(), 0);
    let (term, slot) = tcp(s, &ws.reference, "Term");
    assert_eq!(slot, 1);
    ws.send(&ClientMsg::AddBot);
    (ws, term)
}

fn without_me(mut o: Observation) -> Observation {
    o.me = eksplode_re_core::PlayerId::new(0).unwrap();
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
    let term_start = tcp_game(&term, |m| matches!(m, ServerMsg::Start { .. }));
    let (Some(ServerMsg::Start { seed: a, .. }), Some(ServerMsg::Start { seed: b, .. })) =
        (web_start, term_start)
    else {
        panic!("both must receive Start");
    };
    assert_eq!(a, b);

    let web_first = ws.recv_until(is_snapshot);
    let term_first = tcp_game(&term, is_snapshot);
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
fn web_result(ws: &mut RoomWs) -> (Observation, Outcome) {
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
        match m.into_game_message() {
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
    let left = tcp_game(term, |m| matches!(m, ServerMsg::PlayerLeft { .. }));
    assert_eq!(
        left,
        Some(ServerMsg::PlayerLeft {
            slot,
            replaced_by_bot: true
        })
    );
    let mut ticks = Vec::new();
    while ticks.len() < 2 {
        match tcp_game(term, is_snapshot) {
            Some(ServerMsg::Snapshot { tick, .. }) => ticks.push(tick),
            _ => panic!("ticks stopped after the takeover"),
        }
    }
    assert!(ticks[1] > ticks[0]);
}

fn started_mixed() -> (Server, RoomWs, Client) {
    let s = server(|_| {});
    let (mut ws, term) = mixed(&s);
    ws.send(&ClientMsg::Start);
    assert!(ws.recv_until(is_snapshot).is_some());
    assert!(tcp_game(&term, is_snapshot).is_some());
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
    let s = server(|o| {
        o.silence_timeout = Duration::from_millis(600);
        o.tick = Duration::from_millis(20);
    });
    let term = Client::connect_unjoined(&s.game_addr().to_string()).unwrap();
    let room = term
        .create_room("Term", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    let mut ws = RoomWs::join(s.http_addr(), &room, "Quiet");
    assert_eq!(ws.welcome_slot(), 1);
    term.send(ClientMsg::Start).unwrap();
    let mut tick = 0;
    let mut left = false;
    while let Ok(m) = term.recv_timeout(WAIT) {
        match m.into_game_message() {
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
    let mut a = RoomWs::create(s.http_addr(), "Ann");
    assert_eq!(a.welcome_slot(), 0);
    let (_t1, slot) = tcp(&s, &a.reference, "Ann");
    assert_eq!(slot, 1);
    // two tabs from the same address are two players, and the same name is fine
    let mut b = RoomWs::join(s.http_addr(), &a.reference, "Ann");
    assert_eq!(b.welcome_slot(), 2);
    let (_t2, slot) = tcp(&s, &a.reference, "Bob");
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
    assert!(late.hello_v2().is_some());
    late.join_room_v2("late", &serde_json::to_value(&a.reference).unwrap(), "Late");
    assert_eq!(late.recv_value().unwrap()["code"], "Full");
    let late_tcp = Client::connect_unjoined(&s.game_addr().to_string()).unwrap();
    assert!(matches!(late_tcp.join_room(&a.reference, "Late"),
        Err(eksplode_re_net::codec::NetError::Protocol(code)) if code == "Full"));
}

#[test]
fn us3_as3_cli_join_sees_browser_player() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Webby");
    ws.welcome_slot();
    let c = Client::connect_unjoined(&s.game_addr().to_string()).unwrap();
    c.join_room(&ws.reference, "Termy").unwrap();
    match tcp_wait(&c, |m| {
        matches!(m, ServerMsg::RoomV2(reply)
        if matches!(reply.as_ref(), eksplode_re_net::protocol::v2::Reply::Welcome { .. }))
    })
    .map(ServerMsg::into_game_message)
    {
        Some(ServerMsg::Welcome { slot, lobby, .. }) => {
            assert_eq!(slot, 1);
            assert_eq!(lobby.slots[0].name, "Webby");
        }
        other => panic!("{other:?}"),
    }
}

/// A browser or a terminal participant seen as a message channel.
enum Endpoint {
    Web(Box<RoomWs>),
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
            Endpoint::Term(c) => c.recv_timeout(WAIT).ok().map(ServerMsg::into_game_message),
        }
    }
}

/// Walks a column and back, timing how long each step takes to show up in a snapshot.
fn drive(mut end: Endpoint, slot: usize) -> Vec<Duration> {
    use eksplode_re_core::{Dir, Event};
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
    p95_latency(|_| {});
}

#[test]
fn f4_sc010_key_to_snapshot_latency_on_the_large_board() {
    // Explicit creation uses the server's defaults: the large board.
    p95_latency(|o| {
        o.room = eksplode_re_core::RoomSettings {
            size: eksplode_re_core::BoardSize::Large,
            ..Default::default()
        }
    });
}

fn p95_latency(configure: impl FnOnce(&mut eksplode_re_web::ServerOptions)) {
    // the real 20 Hz tick; a one-tick move cooldown so a step does not wait for the cooldown
    let s = server(|o| {
        o.tick = Duration::from_millis(50);
        o.game.block_density = 0.0;
        o.game.base_move_cooldown = 1;
        o.game.min_move_cooldown = 1;
        configure(o);
    });
    let mut ws0 = RoomWs::create(s.http_addr(), "w0");
    assert_eq!(ws0.welcome_slot(), 0);
    let (t1, slot) = tcp(&s, &ws0.reference, "t1");
    assert_eq!(slot, 1);
    let mut ws2 = RoomWs::join(s.http_addr(), &ws0.reference, "w2");
    assert_eq!(ws2.welcome_slot(), 2);
    let (t3, slot) = tcp(&s, &ws0.reference, "t3");
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

/// What one client saw of a whole match.
struct Seen {
    looks: Vec<eksplode_re_net::look::Look>,
    events: Vec<eksplode_re_core::Event>,
    last: Observation,
    outcome: Outcome,
}

fn watch(mut next: impl FnMut() -> Option<ServerMsg>) -> Seen {
    let mut looks = Vec::new();
    let mut events = Vec::new();
    let mut last = None;
    while let Some(m) = next() {
        match m {
            ServerMsg::Welcome { looks: l, .. } | ServerMsg::Lobby { looks: l, .. }
                if events.is_empty() =>
            {
                looks = l
            }
            ServerMsg::Snapshot {
                state, events: e, ..
            } => {
                events.extend(e);
                last = Some(*state);
            }
            ServerMsg::Ended { outcome } => {
                return Seen {
                    looks,
                    events,
                    last: last.expect("a snapshot"),
                    outcome,
                }
            }
            _ => {}
        }
    }
    panic!("the match never ended");
}

#[test]
fn f3_us2_as9_powers_work_the_same_for_every_client() {
    use eksplode_re_core::Event;
    let s = server(|o| {
        o.seed = Some(3);
        o.game.block_density = 1.0;
        o.game.powerup_chance = 1.0;
        o.game.round_limit_ticks = 120;
    });
    let mut ws = RoomWs::create(s.http_addr(), "Web");
    assert_eq!(ws.welcome_slot(), 0);
    ws.send(&ClientMsg::SetLook {
        look: serde_json::json!({"colour":"Teal","gender":"Female","head":"Crown","face":"Glasses"})
    });
    let (term, slot) = tcp(&s, &ws.reference, "Term");
    assert_eq!(slot, 1);
    ws.send(&ClientMsg::AddBot);
    ws.send(&ClientMsg::Start);
    // both humans drop a bomb right away so that blocks burn and items appear
    ws.send(&ClientMsg::Input {
        tick: 0,
        mv: None,
        bomb: true,
        punch: false,
    });
    term.send(ClientMsg::Input {
        tick: 0,
        mv: None,
        bomb: true,
        punch: false,
    })
    .unwrap();

    let web = watch(|| ws.recv());
    let term = watch(|| {
        term.recv_timeout(WAIT)
            .ok()
            .map(ServerMsg::into_game_message)
    });

    assert_eq!(web.outcome, term.outcome, "the same result");
    assert_eq!(web.events, term.events, "the same events, drops included");
    assert_eq!(
        without_me(web.last),
        without_me(term.last),
        "the same final state"
    );
    // the match saw items appear (every block drops one) and the same looks everywhere
    assert!(
        web.events
            .iter()
            .any(|e| matches!(e, Event::PowerUpSpawned { .. })),
        "no item dropped"
    );
    assert_eq!(web.looks.len(), 4);
    assert_eq!(web.looks, term.looks, "the same looks for everyone");
    assert_eq!(web.looks[0].colour.name(), "Teal");
}
