#![allow(clippy::unwrap_used)]

mod common;

use bomberust_bots::{Bot, DefaultBot};
use bomberust_core::{blast, step, Config, GameState, Outcome, Phase, PlayerInput, Pos};
use common::*;
use std::collections::{BTreeSet, VecDeque};

fn play(seed: u64) -> GameState {
    let mut s = GameState::new(Config::default(), seed, [true; 4]).unwrap();
    let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
    let limit = s.config.round_limit_ticks + 5;
    for _ in 0..limit {
        if s.phase != Phase::Playing {
            break;
        }
        let mut inputs = [PlayerInput::default(); 4];
        for (i, bot) in bots.iter_mut().enumerate() {
            inputs[i] = bot.decide(&s.observe(pid(i as u8)));
        }
        step(&mut s, &inputs);
    }
    s
}

#[test]
fn us2_as4_four_bot_matches_always_finish() {
    let mut winners = 0;
    for seed in 0..100 {
        let s = play(seed);
        match s.phase {
            Phase::Ended(Outcome::Winner(_)) => winners += 1,
            Phase::Ended(Outcome::Draw) => {}
            Phase::Playing => panic!("seed {seed} did not finish within the round limit"),
        }
        assert!(s.tick <= s.config.round_limit_ticks, "seed {seed}");
    }
    println!("100/100 matches finished, {winners} with a winner");
}

/// Is there any tile outside the blast within `max_moves` plain moves?
fn escape_exists(s: &GameState, from: Pos, bomb_range: u8, max_moves: u32) -> bool {
    let danger = blast(&s.arena, from, bomb_range).tiles;
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([(from, 0u32)]);
    while let Some((p, d)) = queue.pop_front() {
        if !danger.contains(&p) {
            return true;
        }
        if d == max_moves {
            continue;
        }
        for dir in bomberust_core::Dir::ALL {
            let Some(n) = p.step(dir, s.arena.width(), s.arena.height()) else {
                continue;
            };
            if s.arena.tile(n) == bomberust_core::Tile::Floor && seen.insert(n) {
                queue.push_back((n, d + 1));
            }
        }
    }
    false
}

/// A bomb sits under the bot at the start; if any escape exists the bot has to survive.
#[test]
fn us2_as2_bots_escape_in_at_least_95_percent_of_scripted_scenarios() {
    let (mut applicable, mut survived) = (0, 0);
    for seed in 0..200u64 {
        let active = [true, true, false, false];
        let open = Config {
            block_density: 0.35,
            ..Config::default()
        };
        let mut s = GameState::new(open, seed, active).unwrap();
        let me = s.players[0].pos;
        let range = s.players[1].range;
        s.bombs.push(bomberust_core::Bomb {
            owner: pid(1),
            pos: me,
            ticks_left: 30,
            range,
            flight: None,
            slide: None,
        });
        s.players[1].bombs_active = 1;
        if !escape_exists(&s, me, range, 5) {
            continue;
        }
        applicable += 1;
        let mut bot = DefaultBot::new(seed);
        for _ in 0..40 {
            let mut inputs = [PlayerInput::default(); 4];
            inputs[0] = bot.decide(&s.observe(pid(0)));
            step(&mut s, &inputs);
        }
        if s.players[0].alive {
            survived += 1;
        }
    }
    assert!(
        applicable >= 100,
        "only {applicable} scenarios had an escape"
    );
    let rate = f64::from(survived) / f64::from(applicable);
    println!("escape rate {survived}/{applicable} = {rate:.3}");
    assert!(rate >= 0.95, "escape rate {rate}");
}
