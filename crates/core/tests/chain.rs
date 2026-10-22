mod common;

use bomberust_core::{step, Dir, Event, PlayerInput};
use common::*;

fn exploded(ev: &[Event]) -> Vec<bomberust_core::Pos> {
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
