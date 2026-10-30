mod common;

use common::*;
use eksplode_re_core::{step, Config, Dir, Event, PlayerInput};

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
fn f10_us1_as1_both_players_enter_a_contested_tile() {
    // Players no longer block each other, so regardless of move-resolution order both
    // players moving onto the same open tile both arrive there.
    let mut starts = only(0, 1, 1);
    starts[1] = Some((3, 1));
    for tick in [0u32, 1, 4, 5] {
        let mut s = state(&OPEN, starts);
        s.tick = tick;
        let mut i = idle();
        i[0] = mv(Dir::Right);
        i[1] = mv(Dir::Left);
        step(&mut s, &i);
        assert_eq!(s.players[0].pos, pos(2, 1), "tick {tick}");
        assert_eq!(s.players[1].pos, pos(2, 1), "tick {tick}");
    }
}

#[test]
fn f10_us1_as1_move_onto_tile_occupied_by_another_player_succeeds() {
    let mut starts = only(0, 1, 1);
    starts[1] = Some((2, 1));
    let mut s = state(&OPEN, starts);
    let mut i = idle();
    i[0] = mv(Dir::Right);
    let ev = step(&mut s, &i);
    assert_eq!(s.players[0].pos, pos(2, 1));
    assert_eq!(s.players[1].pos, pos(2, 1));
    assert!(ev.contains(&Event::Moved {
        player: pid(0),
        from: pos(1, 1),
        to: pos(2, 1)
    }));
}

#[test]
fn f10_edge_three_players_converge_on_one_tile() {
    let mut starts = [None; 4];
    starts[0] = Some((2, 1)); // above the target, moves down
    starts[1] = Some((1, 2)); // left of the target, moves right
    starts[2] = Some((3, 2)); // right of the target, moves left
    let mut s = state(&OPEN, starts);
    let mut i = idle();
    i[0] = mv(Dir::Down);
    i[1] = mv(Dir::Right);
    i[2] = mv(Dir::Left);
    step(&mut s, &i);
    for slot in 0..3 {
        assert_eq!(s.players[slot].pos, pos(2, 2), "slot {slot}");
    }
}
