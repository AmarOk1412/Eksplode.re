#![allow(clippy::unwrap_used)]

mod common;

use bomberust_bots::DangerMap;
use common::*;

fn flame_ticks() -> u32 {
    bomberust_core::Config::default().flame_ticks
}

#[test]
fn bomb_blast_tiles_become_dangerous_when_it_explodes() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    add_bomb(&mut s, 1, 3, 3, 10);
    let d = DangerMap::from_observation(&obs(&s, 0));
    for p in [pos(3, 3), pos(1, 3), pos(5, 3), pos(3, 1), pos(3, 5)] {
        assert_eq!(d.first_danger(p), Some(10), "{p:?}");
    }
    assert_eq!(d.first_danger(pos(2, 2)), None);
    assert!(!d.dangerous_during(pos(3, 1), 1, 9));
    assert!(d.dangerous_during(pos(3, 1), 10, 10));
}

#[test]
fn danger_ends_when_the_flames_burn_out() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    add_bomb(&mut s, 1, 3, 3, 10);
    let d = DangerMap::from_observation(&obs(&s, 0));
    let last = 10 + flame_ticks() - 1;
    assert!(d.dangerous_during(pos(3, 1), last, last));
    assert!(!d.dangerous_during(pos(3, 1), last + 1, last + 20));
    assert!(!d.safe_from(pos(3, 1), 1));
    assert!(d.safe_from(pos(3, 1), last + 1));
}

#[test]
fn chain_reactions_pull_the_explosion_forward() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    add_bomb(&mut s, 1, 3, 3, 5);
    add_bomb(&mut s, 1, 5, 3, 50);
    s.players[1].bombs_max = 2;
    let d = DangerMap::from_observation(&obs(&s, 0));
    assert_eq!(
        d.first_danger(pos(5, 5)),
        Some(5),
        "second bomb goes off with the first"
    );
}

#[test]
fn walls_and_blocks_shield_tiles() {
    let rows = [
        "#######", "#.....#", "#.....#", "#...#.#", "#.....#", "#.....#", "#######",
    ];
    let mut s = duel(&rows, (1, 5), (5, 1));
    add_bomb(&mut s, 1, 3, 3, 10);
    let d = DangerMap::from_observation(&obs(&s, 0));
    assert_eq!(d.first_danger(pos(4, 3)), None);
    assert_eq!(d.first_danger(pos(5, 3)), None);
    assert_eq!(d.first_danger(pos(2, 3)), Some(10));
}

#[test]
fn existing_flames_are_dangerous_until_they_expire() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    s.flames.push(bomberust_core::Flame {
        pos: pos(2, 2),
        ticks_left: 4,
    });
    let d = DangerMap::from_observation(&obs(&s, 0));
    assert!(d.dangerous_during(pos(2, 2), 1, 3));
    assert!(!d.dangerous_during(pos(2, 2), 4, 40));
}

#[test]
fn a_hypothetical_bomb_can_be_added() {
    let s = duel(&OPEN, (3, 3), (5, 1));
    let o = obs(&s, 0);
    let d = DangerMap::with_bomb(&o, pos(3, 3), 2, 61);
    assert_eq!(d.first_danger(pos(3, 3)), Some(61));
    assert_eq!(d.first_danger(pos(4, 3)), Some(61));
    assert_eq!(
        DangerMap::from_observation(&o).first_danger(pos(3, 3)),
        None
    );
}

fn fly(s: &mut bomberust_core::GameState, at: (u8, u8), dir: bomberust_core::Dir, fuse: u32) {
    add_bomb(s, 1, at.0, at.1, fuse);
    let b = s.bombs.last_mut().unwrap();
    b.flight = Some(bomberust_core::Flight {
        dir,
        hop_left: 2,
        origin: pos(at.0, at.1),
    });
}

#[test]
fn a_bomb_in_flight_is_dangerous_where_it_will_land_and_where_it_is() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    fly(&mut s, (2, 3), bomberust_core::Dir::Right, 10);
    let d = DangerMap::from_observation(&obs(&s, 0));
    // it will land on (3, 3): that cross burns
    for p in [pos(3, 3), pos(3, 1), pos(3, 5), pos(5, 3)] {
        assert_eq!(d.first_danger(p), Some(10), "landing cross {p:?}");
    }
    // it could also still be over (2, 3) when the fuse ends
    assert_eq!(
        d.first_danger(pos(2, 2)),
        Some(10),
        "cross of the current square"
    );
    assert_eq!(d.first_danger(pos(1, 1)), None);
}

#[test]
fn the_landing_prediction_hops_over_blocks_players_and_other_bombs() {
    let rows = [
        "#######", "#.....#", "#.....#", "#..+..#", "#.....#", "#.....#", "#######",
    ];
    let mut s = duel(&rows, (1, 5), (4, 3));
    s.arena = bomberust_core::Arena::from_rows(&[
        "#######", "#.....#", "#.....#", "#.+...#", "#.....#", "#.....#", "#######",
    ])
    .unwrap();
    fly(&mut s, (1, 3), bomberust_core::Dir::Right, 12);
    // (2, 3) is a block and (3, 3) free: it lands on (3, 3)
    let landing = bomberust_core::punch::predict_landing(&s.arena, &s.players, &s.bombs, 0);
    assert_eq!(landing, pos(3, 3));
    // a player on (3, 3) moves the prediction on to (4, 3) ... which holds player 1: (5, 3)
    s.players[0].pos = pos(3, 3);
    let landing = bomberust_core::punch::predict_landing(&s.arena, &s.players, &s.bombs, 0);
    assert_eq!(landing, pos(5, 3));
    let d = DangerMap::from_observation(&obs(&s, 0));
    assert_eq!(d.first_danger(pos(5, 3)), Some(12));
}

fn slide(s: &mut bomberust_core::GameState, at: (u8, u8), dir: bomberust_core::Dir, fuse: u32) {
    add_bomb(s, 1, at.0, at.1, fuse);
    let b = s.bombs.last_mut().unwrap();
    b.slide = Some(bomberust_core::Slide { dir, step_left: 2 });
}

#[test]
fn f3_us2_as12_a_sliding_bomb_is_dangerous_where_it_is_and_where_it_will_stop() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    slide(&mut s, (1, 3), bomberust_core::Dir::Right, 10);
    let d = DangerMap::from_observation(&obs(&s, 0));
    // it will stop on (5, 3), the last free square before the wall: that cross burns
    for p in [pos(5, 3), pos(5, 1), pos(5, 5), pos(3, 3)] {
        assert_eq!(d.first_danger(p), Some(10), "stop cross {p:?}");
    }
    // and the cross of the current square burns too
    assert_eq!(d.first_danger(pos(1, 2)), Some(10), "current cross");
    assert_eq!(d.first_danger(pos(1, 1)), Some(10));
}

#[test]
fn f3_us2_as12_the_stop_square_follows_blockers_on_the_way() {
    let mut s = duel(&OPEN, (1, 5), (5, 1));
    s.players[1].pos = pos(4, 3);
    slide(&mut s, (1, 3), bomberust_core::Dir::Right, 12);
    let landing = bomberust_core::kick::predict_slide_stop(&s.arena, &s.players, &s.bombs, 0);
    assert_eq!(landing, pos(3, 3), "it stops before the living player");
    let d = DangerMap::from_observation(&obs(&s, 0));
    assert_eq!(d.first_danger(pos(3, 2)), Some(12));
}
