mod common;

use bomberust_core::{Arena, Config, GameState, PowerUpKind, Tile};
use common::*;

fn corners(c: &Config) -> [(u8, u8); 4] {
    [
        (1, 1),
        (c.width - 2, 1),
        (1, c.height - 2),
        (c.width - 2, c.height - 2),
    ]
}

#[test]
fn us1_as1_border_and_fixed_walls() {
    let c = Config::default();
    for seed in 0..10 {
        let a = GameState::new(c.clone(), seed, [true; 4]).unwrap().arena;
        for y in 0..c.height {
            for x in 0..c.width {
                let border = x == 0 || y == 0 || x == c.width - 1 || y == c.height - 1;
                let fixed = x % 2 == 0 && y % 2 == 0;
                if border || fixed {
                    assert_eq!(a.tile(pos(x, y)), Tile::Wall, "({x},{y}) seed {seed}");
                }
            }
        }
    }
}

#[test]
fn us1_as1_start_areas_are_clear() {
    let c = Config::default();
    for seed in 0..20 {
        let a = GameState::new(c.clone(), seed, [true; 4]).unwrap().arena;
        for (cx, cy) in corners(&c) {
            let dx: i16 = if cx == 1 { 1 } else { -1 };
            let dy: i16 = if cy == 1 { 1 } else { -1 };
            for p in [
                pos(cx, cy),
                pos((i16::from(cx) + dx) as u8, cy),
                pos(cx, (i16::from(cy) + dy) as u8),
            ] {
                assert_eq!(a.tile(p), Tile::Floor, "{p:?} seed {seed}");
            }
        }
    }
}

#[test]
fn us1_as1_players_start_in_distinct_corners() {
    let c = Config::default();
    let s = GameState::new(c.clone(), 3, [true; 4]).unwrap();
    for (i, (x, y)) in corners(&c).into_iter().enumerate() {
        assert_eq!(s.players[i].pos, pos(x, y));
        assert!(s.players[i].alive);
    }
}

#[test]
fn unused_slots_are_not_alive() {
    let s = GameState::new(Config::default(), 3, [true, false, true, false]).unwrap();
    assert!(s.players[0].alive && !s.players[1].alive && s.players[2].alive && !s.players[3].alive);
}

#[test]
fn new_rejects_empty_slots_and_bad_config() {
    assert!(GameState::new(Config::default(), 1, [false; 4]).is_err());
    let bad = Config {
        width: 10,
        ..Config::default()
    };
    assert!(GameState::new(bad, 1, [true; 4]).is_err());
}

#[test]
fn same_seed_same_arena_and_hidden_powerups() {
    let a = GameState::new(Config::default(), 42, [true; 4]).unwrap();
    let b = GameState::new(Config::default(), 42, [true; 4]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn different_seeds_give_different_layouts() {
    let a = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    let b = GameState::new(Config::default(), 2, [true; 4]).unwrap();
    assert_ne!(a.arena, b.arena);
}

#[test]
fn blocks_exist_and_hide_powerups_only_under_blocks() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 9, [true; 4]).unwrap();
    let blocks: Vec<_> = s.arena.positions_of(Tile::Block);
    assert!(!blocks.is_empty());
    assert_eq!(s.hidden_powerups.len(), blocks.len());
    for p in s.hidden_powerups.keys() {
        assert_eq!(s.arena.tile(*p), Tile::Block);
    }
    let none = Config {
        powerup_chance: 0.0,
        ..Config::default()
    };
    assert!(GameState::new(none, 9, [true; 4])
        .unwrap()
        .hidden_powerups
        .is_empty());
}

#[test]
fn all_powerup_kinds_can_be_hidden() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 5, [true; 4]).unwrap();
    for kind in [
        PowerUpKind::ExtraBomb,
        PowerUpKind::Range,
        PowerUpKind::Speed,
    ] {
        assert!(s.hidden_powerups.values().any(|k| *k == kind), "{kind:?}");
    }
}

#[test]
fn from_rows_parses_and_rejects_bad_input() {
    let a = Arena::from_rows(&OPEN).unwrap();
    assert_eq!((a.width(), a.height()), (7, 7));
    assert_eq!(a.tile(pos(0, 0)), Tile::Wall);
    assert_eq!(a.tile(pos(1, 1)), Tile::Floor);
    assert!(Arena::from_rows(&["##", "#"]).is_err());
    assert!(Arena::from_rows(&["#?#"]).is_err());
}

#[test]
fn out_of_bounds_reads_as_wall() {
    let a = Arena::from_rows(&OPEN).unwrap();
    assert_eq!(a.tile(pos(200, 200)), Tile::Wall);
}
