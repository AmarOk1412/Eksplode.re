mod common;

use common::*;
use eksplode_re_core::{step, Config, Dir, Event, GameState, PlayerInput, Pos, Tile};
use std::collections::BTreeSet;

fn flames(s: &GameState) -> BTreeSet<Pos> {
    s.flames.iter().map(|f| f.pos).collect()
}

fn short_fuse() -> Config {
    Config {
        fuse_ticks: 3,
        ..Config::default()
    }
}

/// Two safe players and a fresh bomb of player 0 at (3,3).
fn placed(rows: &[&str], config: Config) -> GameState {
    let fuse = config.fuse_ticks;
    let mut s = state_with(config, rows, two_safe());
    add_bomb(&mut s, 0, 3, 3, fuse);
    s
}

fn run_until_explosion(s: &mut GameState) -> Vec<Event> {
    for _ in 0..100 {
        let ev = step(s, &idle());
        if ev.iter().any(|e| matches!(e, Event::BombExploded { .. })) {
            return ev;
        }
    }
    panic!("bomb never exploded");
}

#[test]
fn us1_as3_bomb_explodes_after_fuse() {
    let mut s = state_with(short_fuse(), &OPEN, only(0, 3, 3));
    let mut i = idle();
    i[0] = bomb_input();
    let ev = step(&mut s, &i);
    assert!(ev.contains(&Event::BombPlaced {
        player: pid(0),
        pos: pos(3, 3)
    }));
    for n in 1..3 {
        let ev = step(&mut s, &idle());
        assert!(
            !ev.iter().any(|e| matches!(e, Event::BombExploded { .. })),
            "step {n}"
        );
    }
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::BombExploded {
        owner: pid(0),
        pos: pos(3, 3)
    }));
    assert!(s.bombs.is_empty());
}

#[test]
fn us1_as3_blast_is_cross_shaped_up_to_range() {
    let mut s = placed(&OPEN, short_fuse());
    run_until_explosion(&mut s);
    let expected: BTreeSet<Pos> = [
        (3, 3),
        (2, 3),
        (1, 3),
        (4, 3),
        (5, 3),
        (3, 2),
        (3, 1),
        (3, 4),
        (3, 5),
    ]
    .into_iter()
    .map(|(x, y)| pos(x, y))
    .collect();
    assert_eq!(flames(&s), expected);
}

#[test]
fn us1_as3_walls_stop_blast() {
    let rows = [
        "#######", "#.....#", "#.....#", "#...#.#", "#.....#", "#.....#", "#######",
    ];
    let mut s = placed(&rows, short_fuse());
    run_until_explosion(&mut s);
    let f = flames(&s);
    assert!(f.contains(&pos(3, 3)));
    assert!(!f.contains(&pos(4, 3)) && !f.contains(&pos(5, 3)));
    assert!(f.contains(&pos(2, 3)) && f.contains(&pos(1, 3)));
}

#[test]
fn us1_as3_first_block_is_destroyed_and_stops_the_blast() {
    let rows = [
        "#######", "#.....#", "#.....#", "#...++#", "#.....#", "#.....#", "#######",
    ];
    let mut s = placed(&rows, short_fuse());
    let ev = run_until_explosion(&mut s);
    assert_eq!(s.arena.tile(pos(4, 3)), Tile::Floor);
    assert_eq!(s.arena.tile(pos(5, 3)), Tile::Block);
    assert!(flames(&s).contains(&pos(4, 3)));
    assert!(!flames(&s).contains(&pos(5, 3)));
    assert!(ev.contains(&Event::BlockDestroyed { pos: pos(4, 3) }));
}

#[test]
fn us1_as3_bomb_cap_and_single_bomb_per_tile() {
    let mut s = state(&OPEN, only(0, 3, 3));
    s.players[0].bombs_max = 2;
    let mut i = idle();
    i[0] = bomb_input();
    step(&mut s, &i);
    step(&mut s, &i);
    assert_eq!(s.bombs.len(), 1, "second bomb on the same tile is ignored");

    let mut s = state(&OPEN, only(0, 3, 3));
    step(&mut s, &{
        let mut i = idle();
        i[0] = bomb_input();
        i
    });
    let mut right = idle();
    right[0] = PlayerInput {
        mv: Some(Dir::Right),
        bomb: true,
        ..PlayerInput::default()
    };
    step(&mut s, &right);
    assert_eq!(s.bombs.len(), 1, "cap of one active bomb");
    assert_eq!(s.players[0].pos, pos(4, 3));
}

#[test]
fn us1_as3_bomb_slot_returns_after_explosion() {
    let mut s = placed(&OPEN, short_fuse());
    assert_eq!(s.players[0].bombs_active, 1);
    run_until_explosion(&mut s);
    assert_eq!(s.players[0].bombs_active, 0);
}

#[test]
fn flames_expire_after_flame_ticks() {
    let c = Config {
        flame_ticks: 3,
        ..short_fuse()
    };
    let mut s = placed(&OPEN, c);
    run_until_explosion(&mut s);
    assert!(!s.flames.is_empty());
    step(&mut s, &idle());
    step(&mut s, &idle());
    assert!(!s.flames.is_empty(), "still burning");
    step(&mut s, &idle());
    assert!(s.flames.is_empty());
}

// ---- 011 US5: bombs placed ----

#[test]
fn f11_us5_bombs_placed_counts_each_successful_placement_once() {
    let mut s = state_with(short_fuse(), &OPEN, only(0, 3, 3));
    s.players[0].bombs_max = 2;
    assert_eq!(s.players[0].bombs_placed, 0, "a match starts at zero");
    let mut i = idle();
    i[0] = bomb_input();
    step(&mut s, &i);
    assert_eq!(s.players[0].bombs_placed, 1);
    // a refused placement (a bomb already lies on the tile) does not count
    step(&mut s, &i);
    assert_eq!(s.bombs.len(), 1);
    assert_eq!(s.players[0].bombs_placed, 1);
    // what happens to the bomb afterwards does not change the count
    run_until_explosion(&mut s);
    assert!(s.bombs.is_empty());
    assert_eq!(s.players[0].bombs_placed, 1);
    // a bomb added without placing it (as the tests do) is not counted either
    add_bomb(&mut s, 0, 1, 1, 50);
    assert_eq!(s.players[0].bombs_placed, 1);
}
