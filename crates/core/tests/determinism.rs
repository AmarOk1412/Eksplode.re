mod common;

use bomberust_core::{step, Config, Dir, GameState, PlayerInput, Rng, Tile};
use common::*;
use std::collections::BTreeSet;

fn random_inputs(rng: &mut Rng) -> [PlayerInput; 4] {
    let mut inputs = idle();
    for input in &mut inputs {
        input.mv = match rng.next_below(5) {
            0 => Some(Dir::Up),
            1 => Some(Dir::Down),
            2 => Some(Dir::Left),
            3 => Some(Dir::Right),
            _ => None,
        };
        input.bomb = rng.chance(0.08);
    }
    inputs
}

fn check_invariants(s: &GameState) {
    let mut seen = BTreeSet::new();
    for p in s.players.iter().filter(|p| p.alive) {
        assert_eq!(s.arena.tile(p.pos), Tile::Floor, "player on a solid tile");
        assert!(seen.insert(p.pos), "two living players share {:?}", p.pos);
    }
    for p in &s.players {
        let owned = s.bombs.iter().filter(|b| b.owner == p.id).count();
        assert_eq!(
            usize::from(p.bombs_active),
            owned,
            "bomb accounting for {:?}",
            p.id
        );
        assert!(p.bombs_active <= p.bombs_max);
        assert!(p.range <= s.config.max_range && p.bombs_max <= s.config.max_bombs);
    }
    let mut bomb_tiles = BTreeSet::new();
    for b in &s.bombs {
        assert!(bomb_tiles.insert(b.pos), "two bombs on {:?}", b.pos);
    }
}

#[test]
fn same_seed_and_inputs_give_identical_results() {
    for seed in 0..50u64 {
        let mut a = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut b = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut ra = Rng::new(seed ^ 0xABCD);
        let mut rb = Rng::new(seed ^ 0xABCD);
        for _ in 0..600 {
            let ea = step(&mut a, &random_inputs(&mut ra));
            let eb = step(&mut b, &random_inputs(&mut rb));
            assert_eq!(ea, eb, "seed {seed}");
        }
        assert_eq!(a, b, "seed {seed}");
    }
}

#[test]
fn random_play_never_panics_and_keeps_invariants() {
    for seed in 0..10u64 {
        let mut s = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut rng = Rng::new(seed);
        for _ in 0..3600 {
            step(&mut s, &random_inputs(&mut rng));
            check_invariants(&s);
        }
    }
}

#[test]
fn the_round_always_ends_within_the_limit() {
    let mut s = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    let mut rng = Rng::new(1);
    for _ in 0..s.config.round_limit_ticks {
        step(&mut s, &random_inputs(&mut rng));
    }
    assert_ne!(s.phase, bomberust_core::Phase::Playing);
}

#[test]
fn observation_hides_rng_and_unrevealed_powerups() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 9, [true; 4]).unwrap();
    assert!(!s.hidden_powerups.is_empty());
    let obs = s.observe(pid(2));
    assert_eq!(obs.me, pid(2));
    assert!(obs.powerups.is_empty());
    assert_eq!(obs.arena, s.arena);
    assert_eq!(obs.players, s.players);
    assert_eq!(obs.tick, s.tick);
}
