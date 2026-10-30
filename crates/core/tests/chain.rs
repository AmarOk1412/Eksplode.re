#![allow(clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{step, Dir, Event, PlayerInput};

fn exploded(ev: &[Event]) -> Vec<eksplode_re_core::Pos> {
    ev.iter()
        .filter_map(|e| match e {
            Event::BombExploded { pos, .. } => Some(*pos),
            _ => None,
        })
        .collect()
}

#[test]
fn us1_as4_bomb_in_blast_detonates_immediately() {
    let mut s = state(&OPEN, two_safe());
    add_bomb(&mut s, 0, 3, 3, 1);
    add_bomb(&mut s, 1, 5, 3, 50);
    let ev = step(&mut s, &idle());
    let mut got = exploded(&ev);
    got.sort();
    assert_eq!(got, vec![pos(3, 3), pos(5, 3)]);
    assert!(s.bombs.is_empty());
    assert_eq!(s.players[1].bombs_active, 0);
}

#[test]
fn us1_as4_chain_settles_across_several_bombs() {
    let mut s = state(&OPEN, two_safe());
    add_bomb(&mut s, 0, 3, 3, 1);
    add_bomb(&mut s, 1, 5, 3, 50);
    add_bomb(&mut s, 1, 5, 5, 50);
    s.players[1].bombs_max = 3;
    let ev = step(&mut s, &idle());
    assert_eq!(exploded(&ev).len(), 3);
    assert!(s.bombs.is_empty());
}

#[test]
fn f10_us1_as3_blast_eliminates_both_players_sharing_a_tile() {
    // Two players stand on the same tile (now allowed, see feature 010); a blast covering
    // that tile must eliminate each of them independently, with no immunity from sharing it.
    let starts = [Some((3, 3)), Some((3, 3)), None, None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 4, 1);
    let ev = step(&mut s, &idle());
    assert!(!s.players[0].alive);
    assert!(!s.players[1].alive);
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(0) }));
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(1) }));
}

#[test]
fn bombs_outside_the_blast_are_untouched() {
    let mut s = state(&OPEN, two_safe());
    add_bomb(&mut s, 0, 3, 3, 1);
    add_bomb(&mut s, 1, 5, 5, 50);
    let ev = step(&mut s, &idle());
    assert_eq!(exploded(&ev), vec![pos(3, 3)]);
    assert_eq!(s.bombs.len(), 1);
}

#[test]
fn us1_as5_player_in_blast_is_eliminated() {
    let starts = [Some((1, 5)), Some((5, 1)), Some((3, 2)), None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 1, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert!(!s.players[2].alive);
    assert!(s.players[0].alive && s.players[1].alive);
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(2) }));
}

#[test]
fn us1_as5_own_blast_eliminates_the_owner() {
    let starts = [Some((3, 3)), Some((1, 5)), Some((5, 1)), None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert!(!s.players[0].alive);
}

#[test]
fn us1_as5_walking_into_active_flames_is_fatal() {
    let starts = [Some((1, 5)), Some((5, 2)), Some((5, 5)), None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert!(s.players[1].alive, "(5,2) is outside the cross");
    let mut i = idle();
    i[1] = PlayerInput {
        mv: Some(Dir::Down),
        bomb: false,
        ..PlayerInput::default()
    };
    let ev = step(&mut s, &i);
    assert!(!s.players[1].alive);
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(1) }));
}

#[test]
fn walls_shield_players_from_blasts() {
    let rows = [
        "#######", "#.....#", "#.....#", "#..#..#", "#.....#", "#.....#", "#######",
    ];
    let starts = [Some((1, 5)), Some((5, 1)), Some((4, 3)), None];
    let mut s = state(&rows, starts);
    add_bomb(&mut s, 0, 2, 3, 1);
    step(&mut s, &idle());
    assert!(s.players[2].alive, "wall at (3,3) blocks the ray to (4,3)");
}

fn owners_at(s: &eksplode_re_core::GameState, x: u8, y: u8) -> Vec<u8> {
    let f = s
        .flames
        .iter()
        .find(|f| f.pos == pos(x, y))
        .expect("a flame");
    (0..4).filter(|i| f.owned_by(pid(*i))).collect()
}

#[test]
fn a_flame_is_owned_by_the_owner_of_its_bomb() {
    let mut s = state(&OPEN, [Some((1, 5)), Some((5, 5)), None, None]);
    add_bomb(&mut s, 1, 3, 3, 1);
    step(&mut s, &idle());
    for (x, y) in [(3, 3), (2, 3), (3, 1), (5, 3)] {
        assert_eq!(owners_at(&s, x, y), vec![1], "flame at {x},{y}");
    }
}

#[test]
fn chain_flames_keep_the_owner_of_the_bomb_that_made_them() {
    let mut s = state(&OPEN, [Some((1, 5)), Some((5, 5)), None, None]);
    add_bomb(&mut s, 0, 1, 1, 1);
    add_bomb(&mut s, 1, 3, 1, 50);
    step(&mut s, &idle());
    assert_eq!(
        owners_at(&s, 1, 2),
        vec![0],
        "only the first bomb reaches it"
    );
    assert_eq!(
        owners_at(&s, 3, 2),
        vec![1],
        "only the chained bomb reaches it"
    );
    assert_eq!(owners_at(&s, 4, 1), vec![1]);
    assert_eq!(owners_at(&s, 2, 1), vec![0, 1], "both bombs reach it");
    assert_eq!(owners_at(&s, 3, 1), vec![0, 1]);
}

// ---- 011 US5: kills and elimination order ----

#[test]
fn f11_us5_as1_a_kill_is_credited_to_the_bomb_owner_and_stamps_the_order() {
    let mut s = state(&OPEN, [Some((5, 1)), Some((3, 3)), None, None]);
    assert_eq!(s.players[1].eliminated_order, None);
    add_bomb(&mut s, 0, 3, 4, 1);
    step(&mut s, &idle());
    assert!(!s.players[1].alive);
    assert_eq!(s.players[1].eliminated_order, Some(1));
    assert_eq!(s.players[0].kills, 1);
    assert_eq!(s.players[0].eliminated_order, None, "the survivor has none");
    assert_eq!(s.players[1].kills, 0);
}

#[test]
fn f11_us5_a_self_elimination_credits_nobody() {
    let mut s = state(&OPEN, [Some((3, 3)), Some((5, 1)), None, None]);
    add_bomb(&mut s, 0, 3, 4, 1);
    step(&mut s, &idle());
    assert!(!s.players[0].alive);
    assert_eq!(s.players[0].eliminated_order, Some(1));
    assert_eq!(s.players[0].kills, 0);
    assert_eq!(s.players[1].kills, 0);
}

#[test]
fn f11_us5_as4_players_caught_in_one_tick_share_their_order_and_later_ones_come_after() {
    // 0 and 1 share (3,3) and are caught by 2's bomb; 2 is caught later by 3's bomb
    let starts = [Some((3, 3)), Some((3, 3)), Some((5, 1)), Some((1, 1))];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 2, 3, 4, 1);
    add_bomb(&mut s, 3, 5, 2, 3);
    step(&mut s, &idle());
    assert_eq!(s.players[0].eliminated_order, Some(1));
    assert_eq!(
        s.players[1].eliminated_order,
        Some(1),
        "same tick, same order"
    );
    assert_eq!(s.players[2].kills, 2);
    for _ in 0..3 {
        step(&mut s, &idle());
    }
    assert!(!s.players[2].alive);
    assert_eq!(s.players[2].eliminated_order, Some(2));
    assert_eq!(s.players[3].kills, 1);
    assert_eq!(s.players[3].eliminated_order, None);
}

#[test]
fn f11_us5_every_bomb_whose_flame_reaches_the_victim_is_credited() {
    // the bombs of 0 and 1 both light (3,3), where 2 stands
    let starts = [Some((1, 1)), Some((5, 5)), Some((3, 3)), None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 4, 1);
    add_bomb(&mut s, 1, 4, 3, 1);
    step(&mut s, &idle());
    assert!(!s.players[2].alive);
    assert_eq!((s.players[0].kills, s.players[1].kills), (1, 1));
}
