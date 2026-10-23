mod common;

use std::time::{Duration, Instant};

use bomberust_core::{Config, Dir, Event, Outcome};
use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_net::Client;
use common::*;

/// Walks a column away from its corner and back, timing how long each step takes to show up in a snapshot.
fn drive(c: Client, slot: usize) -> (Vec<Duration>, Option<Outcome>, u32) {
    let (first, back) = if slot < 2 {
        (Dir::Down, Dir::Up)
    } else {
        (Dir::Up, Dir::Down)
    };
    let dirs: Vec<Dir> = [first; 5].into_iter().chain([back; 5]).collect();
    let (mut latencies, mut idx, mut sent_at) = (Vec::new(), 0, None::<Instant>);
    let mut last_tick = 0;
    while let Some(m) = wait_for(&c, |_| true) {
        match m {
            ServerMsg::Snapshot { tick, events, .. } => {
                last_tick = tick;
                let moved = events
                    .iter()
                    .any(|e| matches!(e, Event::Moved { player, .. } if player.index() == slot));
                if let (true, Some(t0)) = (moved, sent_at) {
                    latencies.push(t0.elapsed());
                    sent_at = None;
                    idx += 1;
                }
                let mv = dirs.get(idx).copied();
                if mv.is_some() && sent_at.is_none() {
                    sent_at = Some(Instant::now());
                }
                let _ = c.send(ClientMsg::Input {
                    tick,
                    mv,
                    bomb: false,
                    punch: false,
                });
            }
            ServerMsg::Ended { outcome } => return (latencies, Some(outcome), last_tick),
            _ => {}
        }
    }
    (latencies, None, last_tick)
}

#[test]
fn sc006_sc007_actions_show_up_quickly_and_everyone_agrees_on_the_result() {
    let host = host_with(|c| {
        c.game = Config {
            block_density: 0.0,
            ..quick_game(400)
        };
    });
    let clients = join_all(&host, 4);
    host.start_match();
    let handles: Vec<_> = clients
        .into_iter()
        .enumerate()
        .map(|(slot, c)| std::thread::spawn(move || drive(c, slot)))
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("client thread"))
        .collect();

    let mut all: Vec<Duration> = results.iter().flat_map(|r| r.0.clone()).collect();
    for (i, r) in results.iter().enumerate() {
        println!("client {i}: {} moves, last tick {}", r.0.len(), r.2);
    }
    assert!(all.len() >= 36, "only {} moves were observed", all.len());
    all.sort();
    let p95 = all[(all.len() * 95 / 100).min(all.len() - 1)];
    println!("{} samples, p95 latency {:?}", all.len(), p95);
    assert!(p95 < Duration::from_millis(200), "p95 latency {p95:?}");

    let outcome = results[0].1.expect("the round ended");
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r.1, Some(outcome), "client {i} disagrees on the outcome");
        assert_eq!(r.2, results[0].2, "client {i} stopped at a different tick");
    }
}
