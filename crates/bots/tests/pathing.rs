mod common;

use common::*;
use eksplode_re_bots::{find_path, DangerMap, PathRequest};
use eksplode_re_core::Dir;

fn path(
    s: &eksplode_re_core::GameState,
    goal: &dyn Fn(eksplode_re_core::Pos) -> bool,
) -> Option<Vec<Dir>> {
    let o = obs(s, 0);
    let d = DangerMap::from_observation(&o);
    find_path(&PathRequest::new(&o, &d), goal)
}

#[test]
fn finds_the_nearest_safe_tile() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 2, 20);
    let p = path(&s, &|_| true).expect("a way out");
    assert_eq!(p.len(), 1);
    assert!(matches!(p[0], Dir::Left | Dir::Right));
}

#[test]
fn no_path_is_needed_when_already_safe() {
    let s = duel(&OPEN, (3, 3), (5, 5));
    assert_eq!(path(&s, &|_| true), Some(vec![]));
}

#[test]
fn bombs_block_but_players_do_not() {
    let rows = [
        "#######", "#.....#", "#.....#", "#.+.+.#", "#.....#", "#.....#", "#######",
    ];
    let mut s = duel(&rows, (3, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 2, 20);
    let p = path(&s, &|_| true).expect("down and around");
    assert_eq!(p[0], Dir::Down);
    assert_eq!(p.len(), 2);

    // The enemy now stands on the only open escape tile; a living player is not an
    // obstacle (see feature 010), so the bot must still find the same first step.
    let mut s2 = duel(&rows, (3, 3), (3, 4));
    add_bomb(&mut s2, 1, 3, 2, 20);
    let p2 = path(&s2, &|_| true).expect("a living player is not an obstacle");
    assert_eq!(p2[0], Dir::Down, "the enemy's tile is a valid first step");
}

#[test]
fn only_paths_that_arrive_before_the_blast_count() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 3, 2);
    assert_eq!(
        path(&s, &|_| true),
        None,
        "two moves take longer than two ticks"
    );

    let mut s = duel(&OPEN, (3, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 3, 10);
    assert_eq!(path(&s, &|_| true).map(|p| p.len()), Some(2));
}

#[test]
fn a_trapped_player_has_no_path() {
    let rows = [
        "#######", "#.+...#", "#+....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let mut s = duel(&rows, (1, 1), (5, 5));
    add_bomb(&mut s, 0, 1, 1, 5);
    assert_eq!(path(&s, &|_| true), None);
}

#[test]
fn the_move_cooldown_delays_the_first_step() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    s.players[0].cooldown_left = 3;
    add_bomb(&mut s, 1, 3, 3, 4);
    assert_eq!(path(&s, &|_| true), None);

    let mut s = duel(&OPEN, (3, 3), (5, 5));
    s.players[0].cooldown_left = 3;
    add_bomb(&mut s, 1, 3, 3, 8);
    assert_eq!(path(&s, &|_| true).map(|p| p.len()), Some(2));
}

#[test]
fn goals_are_respected() {
    let s = duel(&OPEN, (1, 1), (5, 5));
    let p = path(&s, &|t| t == pos(3, 1)).expect("reachable");
    assert_eq!(p, vec![Dir::Right, Dir::Right]);
}

#[test]
fn f4_a_path_never_ends_on_an_announced_square() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    for (x, y) in [(3, 3), (3, 2), (2, 3), (4, 3), (3, 4)] {
        s.closing.falling.push(eksplode_re_core::Falling {
            pos: pos(x, y),
            ticks_left: 30,
        });
    }
    let o = obs(&s, 0);
    let d = DangerMap::from_observation(&o);
    let req = PathRequest::new(&o, &d);
    let path = find_path(&req, &|_| true).expect("a way out of the cross");
    let mut at = pos(3, 3);
    for dir in &path {
        at = at.step(*dir, 7, 7).unwrap();
    }
    assert!(
        s.closing.falling.iter().all(|f| f.pos != at),
        "ended on announced square {at:?}"
    );
}
