mod common;

use bomberust_core::{step, Config, Dir, Event, PlayerInput, PowerUp, PowerUpKind};
use common::*;

fn go(d: Dir) -> [PlayerInput; 4] {
    let mut i = idle();
    i[0] = PlayerInput {
        mv: Some(d),
        bomb: false,
    };
    i
}

const WITH_BLOCK: [&str; 7] = [
    "#######", "#.....#", "#.....#", "#...+.#", "#.....#", "#.....#", "#######",
];

#[test]
fn us1_as6_destroyed_block_reveals_hidden_powerup() {
    let mut s = state(&WITH_BLOCK, two_safe());
    s.hidden_powerups.insert(pos(4, 3), PowerUpKind::Range);
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::PowerUpSpawned {
        pos: pos(4, 3),
        kind: PowerUpKind::Range
    }));
    assert_eq!(
        s.powerups,
        vec![PowerUp {
            pos: pos(4, 3),
            kind: PowerUpKind::Range
        }]
    );
    assert!(s.hidden_powerups.is_empty());
}

#[test]
fn destroyed_block_without_hidden_powerup_reveals_nothing() {
    let mut s = state(&WITH_BLOCK, two_safe());
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert!(s.powerups.is_empty());
}

#[test]
fn us1_as6_flames_destroy_visible_powerups() {
    let mut s = state(&OPEN, two_safe());
    s.powerups.push(PowerUp {
        pos: pos(2, 3),
        kind: PowerUpKind::Speed,
    });
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::PowerUpDestroyed { pos: pos(2, 3) }));
    assert!(s.powerups.is_empty());
}

#[test]
fn us1_as6_walking_onto_a_powerup_improves_the_player() {
    let mut s = state(&OPEN, only(0, 1, 1));
    s.powerups.push(PowerUp {
        pos: pos(2, 1),
        kind: PowerUpKind::ExtraBomb,
    });
    let ev = step(&mut s, &go(Dir::Right));
    assert_eq!(s.players[0].bombs_max, 2);
    assert!(s.powerups.is_empty());
    assert!(ev.contains(&Event::PowerUpCollected {
        player: pid(0),
        pos: pos(2, 1),
        kind: PowerUpKind::ExtraBomb
    }));

    let mut s = state(&OPEN, only(0, 1, 1));
    s.powerups.push(PowerUp {
        pos: pos(2, 1),
        kind: PowerUpKind::Range,
    });
    step(&mut s, &go(Dir::Right));
    assert_eq!(s.players[0].range, 3);
}

#[test]
fn us1_as6_speed_powerup_shortens_the_cooldown() {
    let mut s = state(&OPEN, only(0, 1, 1));
    s.powerups.push(PowerUp {
        pos: pos(2, 1),
        kind: PowerUpKind::Speed,
    });
    step(&mut s, &go(Dir::Right));
    assert_eq!(s.players[0].move_cooldown, 3);
}

#[test]
fn powerups_respect_the_caps() {
    let c = Config::default();
    let mut s = state(&OPEN, only(0, 1, 1));
    s.players[0].bombs_max = c.max_bombs;
    s.players[0].range = c.max_range;
    s.players[0].move_cooldown = c.min_move_cooldown;
    for (x, kind) in [
        (2, PowerUpKind::ExtraBomb),
        (3, PowerUpKind::Range),
        (4, PowerUpKind::Speed),
    ] {
        s.powerups.push(PowerUp {
            pos: pos(x, 1),
            kind,
        });
    }
    for _ in 0..12 {
        step(&mut s, &go(Dir::Right));
    }
    assert!(s.powerups.is_empty(), "all three collected");
    assert_eq!(s.players[0].bombs_max, c.max_bombs);
    assert_eq!(s.players[0].range, c.max_range);
    assert_eq!(s.players[0].move_cooldown, c.min_move_cooldown);
}

#[test]
fn bigger_range_extends_the_blast() {
    let mut s = state(&OPEN, two_safe());
    add_bomb(&mut s, 0, 3, 3, 1);
    s.bombs[0].range = 1;
    step(&mut s, &idle());
    assert_eq!(s.flames.len(), 5);
}
