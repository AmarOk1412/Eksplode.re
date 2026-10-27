mod common;

use common::*;
use eksplode_re_core::{Arena, Config, GameState, PowerUpKind, Tile};

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
fn same_seed_same_arena() {
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
fn blocks_exist_and_nothing_is_on_the_board_before_a_block_breaks() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 9, [true; 4]).unwrap();
    assert!(!s.arena.positions_of(Tile::Block).is_empty());
    assert!(s.powerups.is_empty(), "items are decided when blocks break");
    assert!(
        (s.drops.total() - 1.0).abs() < 1e-5,
        "the table follows powerup_chance"
    );
    let none = Config {
        powerup_chance: 0.0,
        ..Config::default()
    };
    assert_eq!(
        GameState::new(none, 9, [true; 4]).unwrap().drops.total(),
        0.0
    );
}

#[test]
fn all_powerup_kinds_can_drop() {
    let c = Config {
        powerup_chance: 1.0,
        ..Config::default()
    };
    let s = GameState::new(c, 5, [true; 4]).unwrap();
    for kind in PowerUpKind::ALL {
        assert!(s.drops.chance(kind) > 0.0, "{kind:?}");
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

/// Squares reachable from `from` over tiles accepted by `open`.
fn reach(
    s: &eksplode_re_core::GameState,
    from: eksplode_re_core::Pos,
    open: impl Fn(Tile) -> bool,
) -> usize {
    let mut seen = std::collections::BTreeSet::from([from]);
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(p) = queue.pop_front() {
        for d in eksplode_re_core::Dir::ALL {
            let Some(n) = p.step(d, s.arena.width(), s.arena.height()) else {
                continue;
            };
            if open(s.arena.tile(n)) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    seen.len()
}

#[test]
fn f4_us1_as6_the_chosen_size_gives_a_fair_reachable_arena_with_the_players_in_the_corners() {
    use eksplode_re_core::{BoardSize, RoomSettings};
    for size in BoardSize::ALL {
        let (w, h) = size.dimensions();
        let config = RoomSettings {
            size,
            ..RoomSettings::default()
        }
        .to_config(&Config::default());
        assert_eq!((config.width, config.height), (w, h));
        assert!(w % 2 == 1 && h % 2 == 1, "{size:?} has odd sides");
        for seed in 1..=50u64 {
            let s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
            assert_eq!((s.arena.width(), s.arena.height()), (w, h));
            let at = |x: u8, y: u8| eksplode_re_core::Pos::new(x, y);
            for y in 0..h {
                for x in 0..w {
                    let border = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                    let pillar = x % 2 == 0 && y % 2 == 0;
                    if border || pillar {
                        assert_eq!(s.arena.tile(at(x, y)), Tile::Wall, "{size:?} {x},{y}");
                    }
                }
            }
            let corners = [at(1, 1), at(w - 2, 1), at(1, h - 2), at(w - 2, h - 2)];
            let non_wall = (0..h)
                .flat_map(|y| (0..w).map(move |x| at(x, y)))
                .filter(|p| s.arena.tile(*p) != Tile::Wall)
                .count();
            for (slot, corner) in corners.iter().enumerate() {
                assert_eq!(
                    s.players[slot].pos, *corner,
                    "{size:?} slot {slot} starts in its corner"
                );
                // the corner and its two neighbours are free
                let dx: i16 = if corner.x == 1 { 1 } else { -1 };
                let dy: i16 = if corner.y == 1 { 1 } else { -1 };
                for p in [
                    *corner,
                    at((i16::from(corner.x) + dx) as u8, corner.y),
                    at(corner.x, (i16::from(corner.y) + dy) as u8),
                ] {
                    assert_eq!(
                        s.arena.tile(p),
                        Tile::Floor,
                        "{size:?} seed {seed} start area {p:?}"
                    );
                }
                // every square that is not a wall can be reached from every start (blocks can be
                // blown up), and a player is never closed in by blocks
                assert_eq!(
                    reach(&s, *corner, |t| t != Tile::Wall),
                    non_wall,
                    "{size:?} seed {seed} slot {slot}"
                );
                assert!(
                    reach(&s, *corner, |t| t == Tile::Floor) >= 3,
                    "{size:?} seed {seed} closed in"
                );
            }
        }
    }
}
