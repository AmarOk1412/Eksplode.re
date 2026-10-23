#![allow(clippy::unwrap_used)]

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
fn the_round_always_ends_within_the_limit_plus_the_last_announcement() {
    let mut s = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    let mut rng = Rng::new(1);
    // the end pattern covers the board within the limit plus the announcement of the last block
    for _ in 0..s.config.round_limit_ticks + s.config.closing_lead_ticks + 1 {
        step(&mut s, &random_inputs(&mut rng));
    }
    assert_ne!(s.phase, bomberust_core::Phase::Playing);
}

#[test]
fn observation_shows_no_rng_and_no_drop_table() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 9, [true; 4]).unwrap();
    assert!(s.powerups.is_empty());
    let obs = s.observe(pid(2));
    assert_eq!(obs.me, pid(2));
    assert!(obs.powerups.is_empty());
    assert_eq!(obs.arena, s.arena);
    assert_eq!(obs.players, s.players);
    assert_eq!(obs.tick, s.tick);
}

/// Tiles of the arena as text rows (`#` wall, `b` block, `.` floor).
fn arena_text(s: &GameState) -> String {
    let mut out = String::new();
    for y in 0..s.arena.height() {
        for x in 0..s.arena.width() {
            out.push(match s.arena.tile(bomberust_core::Pos::new(x, y)) {
                Tile::Wall => '#',
                Tile::Block => 'b',
                Tile::Floor => '.',
            });
        }
        out.push('\n');
    }
    out
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

/// Fingerprint of the arena and of 300 ticks of scripted play with the default settings.
fn fingerprint(seed: u64) -> (u64, u64) {
    let mut s = GameState::new(Config::default(), seed, [true; 4]).unwrap();
    let arena = fnv(&arena_text(&s));
    let mut rng = Rng::new(seed ^ 0x5EED);
    let mut log = String::new();
    for _ in 0..300 {
        log.push_str(&format!("{:?}\n", step(&mut s, &random_inputs(&mut rng))));
    }
    (arena, fnv(&log))
}

#[test]
fn f4_sc003_default_settings_play_as_before() {
    // Recorded before feature 004 changed the rules; the arena and the events must not change.
    let golden = [
        (1u64, (9471797745044963153u64, 9582954862015796507u64)),
        (7, (12377994668071421445, 4379125904272061920)),
        (42, (2879090803170658785, 3367478786402810030)),
    ];
    let now: Vec<_> = golden
        .iter()
        .map(|(seed, _)| (*seed, fingerprint(*seed)))
        .collect();
    assert_eq!(now, golden.to_vec());
}

#[test]
fn f4_us2_as7_the_same_seed_settings_and_inputs_give_the_same_pattern_and_result() {
    let config = Config {
        round_limit_ticks: 600,
        closing_ticks: 300,
        ..Config::default()
    };
    let mut started = 0;
    for seed in 0..20u64 {
        let mut a = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut b = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut ra = Rng::new(seed ^ 0x77);
        let mut rb = Rng::new(seed ^ 0x77);
        for _ in 0..700 {
            // walking only, so that everybody is alive when the pattern starts
            let (mut ia, mut ib) = (random_inputs(&mut ra), random_inputs(&mut rb));
            for input in ia.iter_mut().chain(ib.iter_mut()) {
                input.bomb = false;
            }
            let ea = step(&mut a, &ia);
            let eb = step(&mut b, &ib);
            assert_eq!(ea, eb, "seed {seed}");
            if ea.contains(&bomberust_core::Event::PatternStarted) {
                started += 1;
            }
        }
        assert_eq!(a, b, "seed {seed}");
        assert_ne!(
            a.phase,
            bomberust_core::Phase::Playing,
            "seed {seed} must end"
        );
    }
    assert!(started > 0, "the pattern ran in at least one round");
}

#[test]
fn f4_every_mode_and_size_is_deterministic_for_a_seed_and_the_inputs() {
    use bomberust_core::{BoardSize, Mode, RoomSettings};
    for mode in Mode::ALL {
        for size in BoardSize::ALL {
            let config = RoomSettings {
                mode,
                size,
                minutes: 2,
                ..RoomSettings::default()
            }
            .to_config(&Config::default());
            for seed in [1u64, 2, 3] {
                let mut a = GameState::new(config.clone(), seed, [true; 4]).unwrap();
                let mut b = GameState::new(config.clone(), seed, [true; 4]).unwrap();
                let mut ra = Rng::new(seed ^ 0xF4);
                let mut rb = Rng::new(seed ^ 0xF4);
                for _ in 0..config.round_limit_ticks + 40 {
                    let ea = step(&mut a, &random_inputs(&mut ra));
                    let eb = step(&mut b, &random_inputs(&mut rb));
                    assert_eq!(ea, eb, "{mode:?} {size:?} seed {seed}");
                }
                assert_eq!(a, b, "{mode:?} {size:?} seed {seed}");
                assert_ne!(
                    a.phase,
                    bomberust_core::Phase::Playing,
                    "{mode:?} {size:?} seed {seed} must end"
                );
            }
        }
    }
}
