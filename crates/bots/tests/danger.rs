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
