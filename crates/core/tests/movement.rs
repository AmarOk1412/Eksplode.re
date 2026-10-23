mod common;

use bomberust_core::{step, Config, Dir, Event, PlayerInput};
use common::*;

fn mv(d: Dir) -> PlayerInput {
    PlayerInput {
        mv: Some(d),
        bomb: false,
        ..PlayerInput::default()
    }
}

#[test]
fn us1_as2_move_into_open_tile() {
    let mut s = state(&OPEN, only(0, 1, 1));
    let mut i = idle();
    i[0] = mv(Dir::Right);
    let ev = step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(2, 1));
    assert!(ev.contains(&Event::Moved {
        player: pid(0),
        from: pos(1, 1),
        to: pos(2, 1)
    }));
}

#[test]
fn us1_as2_wall_blocks_but_changes_facing() {
    let mut s = state(&OPEN, only(0, 1, 1));
    let mut i = idle();
    i[0] = mv(Dir::Up);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(1, 1));
    assert_eq!(s.players[0].facing, Dir::Up);
}

#[test]
fn us1_as2_block_blocks() {
    let rows = [
        "#######", "#.+...#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let mut s = state(&rows, only(0, 1, 1));
    let mut i = idle();
    i[0] = mv(Dir::Right);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(1, 1));
}

#[test]
fn us1_as2_bomb_blocks() {
    let mut s = state(&OPEN, only(0, 1, 1));
    // place a bomb, step off, try to return
    let mut i = idle();
    i[0] = bomb_input();
    step(&mut s, &i);
    let mut i = idle();
    i[0] = mv(Dir::Right);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(2, 1));
    for _ in 0..4 {
        step(&mut s, &idle());
    }
    let mut i = idle();
    i[0] = mv(Dir::Left);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(2, 1), "own bomb blocks re-entry");
}

#[test]
fn us1_as2_living_player_blocks() {
    let mut starts = only(0, 1, 1);
    starts[1] = Some((2, 1));
    let mut s = state(&OPEN, starts);
    let mut i = idle();
    i[0] = mv(Dir::Right);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(1, 1));
}

#[test]
fn us1_as2_dead_player_cannot_move() {
    let mut s = state(&OPEN, only(0, 1, 1));
    s.players[0].alive = false;
    let mut i = idle();
    i[0] = mv(Dir::Right);
    step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(1, 1));
}

#[test]
fn cooldown_limits_move_rate() {
    let c = Config::default();
    let mut s = state(&OPEN, only(0, 1, 1));
    let mut i = idle();
    i[0] = mv(Dir::Right);
    let mut moves = Vec::new();
    for t in 0..(c.base_move_cooldown * 2) {
        let ev = step(&mut s, &i);
        if ev.iter().any(|e| matches!(e, Event::Moved { .. })) {
            moves.push(t);
        }
    }
    assert_eq!(moves, vec![0, c.base_move_cooldown]);
}

#[test]
fn simultaneous_entry_is_resolved_by_rotating_order() {
    let mut starts = only(0, 1, 1);
    starts[1] = Some((3, 1));
    for (tick, winner) in [(0u32, 0usize), (1, 1), (4, 0), (5, 1)] {
        let mut s = state(&OPEN, starts);
        s.tick = tick;
        let mut i = idle();
        i[0] = mv(Dir::Right);
        i[1] = mv(Dir::Left);
        step(&mut s, &i);
        assert_eq!(s.players[winner].pos, pos(2, 1), "tick {tick}");
        let loser = 1 - winner;
        assert_ne!(s.players[loser].pos, pos(2, 1), "tick {tick}");
    }
}
