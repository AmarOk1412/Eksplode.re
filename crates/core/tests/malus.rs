mod common;

use bomberust_core::state::ActiveMalus;
use bomberust_core::{
    step, Config, Dir, Event, Flame, GameState, PlayerInput, PowerUp, PowerUpKind,
};
use common::*;

const CORRIDOR: [&str; 7] = [
    "#########################",
    "#.......................#",
    "#.......................#",
    "#.......................#",
    "#.......................#",
    "#.......................#",
    "#########################",
];

fn go(d: Dir) -> [PlayerInput; 4] {
    let mut i = idle();
    i[0] = PlayerInput {
        mv: Some(d),
        ..PlayerInput::default()
    };
    i
}

fn solo(x: u8, y: u8) -> GameState {
    state(&CORRIDOR, only(0, x, y))
}

/// Puts `kind` on the tile right of the player and walks onto it.
fn pick_up(s: &mut GameState, kind: PowerUpKind) -> Vec<Event> {
    let p = s.players[0].pos;
    let at = pos(p.x + 1, p.y);
    s.powerups.push(PowerUp { pos: at, kind });
    s.players[0].cooldown_left = 0;
    step(s, &go(Dir::Right))
}

fn moves_in(s: &mut GameState, ticks: u32, dir: Dir) -> usize {
    let mut moves = 0;
    for _ in 0..ticks {
        let events = step(s, &go(dir));
        moves += events
            .iter()
            .filter(|e| matches!(e, Event::Moved { player, .. } if player.index() == 0))
            .count();
    }
    moves
}

#[test]
fn f3_us2_as1_a_pickup_removes_the_item_and_applies_at_once() {
    for kind in PowerUpKind::ALL {
        let mut s = solo(1, 3);
        let events = pick_up(&mut s, kind);
        assert!(s.powerups.is_empty(), "{kind:?} leaves the board");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::PowerUpCollected { kind: k, .. } if *k == kind)),
            "{kind:?}"
        );
        match kind {
            PowerUpKind::ExtraBomb => assert_eq!(s.players[0].bombs_max, 2),
            PowerUpKind::Range => assert_eq!(s.players[0].range, 3),
            PowerUpKind::Speed => assert_eq!(s.players[0].move_cooldown, 3),
            PowerUpKind::Punch => assert!(s.players[0].has_punch),
            _ => assert_eq!(s.players[0].malus.map(|m| m.kind), Some(kind)),
        }
    }
}

#[test]
fn f3_us2_as2_roller_and_flame_stack_up_to_their_limits() {
    let mut s = solo(1, 3);
    pick_up(&mut s, PowerUpKind::Speed);
    pick_up(&mut s, PowerUpKind::Speed);
    assert_eq!(s.players[0].move_cooldown, 2);
    pick_up(&mut s, PowerUpKind::Speed);
    assert_eq!(
        s.players[0].move_cooldown, s.config.min_move_cooldown,
        "no faster than the floor"
    );

    let mut s = solo(1, 3);
    for _ in 0..10 {
        pick_up(&mut s, PowerUpKind::Range);
    }
    assert_eq!(s.players[0].range, s.config.max_range);
    let mut s = solo(1, 3);
    for _ in 0..10 {
        pick_up(&mut s, PowerUpKind::ExtraBomb);
    }
    assert_eq!(s.players[0].bombs_max, s.config.max_bombs);
}

#[test]
fn f3_us2_as6_slow_makes_the_player_move_a_quarter_slower_for_ten_seconds() {
    let base = Config::default();
    let mut normal = solo(1, 3);
    let normal_moves = moves_in(&mut normal, 24, Dir::Right);
    let mut slow = solo(1, 3);
    slow.players[0].malus = Some(ActiveMalus {
        kind: PowerUpKind::Slow,
        ticks_left: base.malus_ticks,
    });
    let slow_moves = moves_in(&mut slow, 24, Dir::Right);
    assert_eq!(normal_moves, 6, "a move every 4 ticks");
    assert_eq!(slow_moves, 4, "a move every ceil(4 * 4 / 3) = 6 ticks");
    assert_eq!(slow.players[0].effective_cooldown(), 6);
}

#[test]
fn f3_us2_as7_hyper_speed_halves_the_cooldown() {
    let base = Config::default();
    let mut hyper = solo(1, 3);
    hyper.players[0].malus = Some(ActiveMalus {
        kind: PowerUpKind::HyperSpeed,
        ticks_left: base.malus_ticks,
    });
    assert_eq!(
        moves_in(&mut hyper, 24, Dir::Right),
        12,
        "a move every 2 ticks"
    );
    assert_eq!(hyper.players[0].effective_cooldown(), 2);
    // never below one tick, even with the roller
    let mut fast = solo(1, 3);
    fast.players[0].move_cooldown = 2;
    fast.players[0].malus = hyper.players[0].malus;
    assert_eq!(fast.players[0].effective_cooldown(), 1);
    fast.players[0].move_cooldown = 1;
    assert_eq!(fast.players[0].effective_cooldown(), 1);
}

#[test]
fn f3_us2_as6_and_as7_a_malus_lasts_exactly_malus_ticks_and_then_the_speed_is_normal() {
    assert_eq!(Config::default().malus_ticks, 200, "10 seconds at 20 Hz");
    for kind in [
        PowerUpKind::Slow,
        PowerUpKind::HyperSpeed,
        PowerUpKind::SmallBomb,
    ] {
        let mut s = solo(1, 3);
        pick_up(&mut s, kind);
        assert_eq!(s.players[0].malus.unwrap().ticks_left, 200);
        let mut ended = Vec::new();
        for n in 1..=200 {
            let events = step(&mut s, &idle());
            if events.iter().any(|e| matches!(e, Event::MalusEnded { .. })) {
                ended.push(n);
            }
            if n < 200 {
                assert!(
                    s.players[0].malus.is_some(),
                    "{kind:?} still active after {n} ticks"
                );
            }
        }
        assert_eq!(
            ended,
            vec![200],
            "{kind:?} ends in the 200th tick after the pickup"
        );
        assert!(s.players[0].malus.is_none());
        assert_eq!(
            s.players[0].effective_cooldown(),
            s.players[0].move_cooldown
        );
    }
}

#[test]
fn f3_us2_as8_small_bomb_gives_new_bombs_a_reach_of_one_and_leaves_earlier_bombs_alone() {
    let mut s = solo(1, 3);
    s.players[0].range = 4;
    s.players[0].bombs_max = 3;
    let mut input = idle();
    input[0].bomb = true;
    step(&mut s, &input);
    assert_eq!(s.bombs[0].range, 4, "a bomb placed before the malus");
    pick_up(&mut s, PowerUpKind::SmallBomb);
    let mut input = idle();
    input[0].bomb = true;
    step(&mut s, &input);
    assert_eq!(s.bombs.len(), 2);
    assert_eq!(s.bombs[1].range, 1, "a small bomb reaches one square");
    assert_eq!(s.bombs[0].range, 4);
}

#[test]
fn after_the_small_bomb_malus_ends_bombs_have_their_normal_reach_again() {
    let mut s = solo(1, 3);
    s.players[0].range = 4;
    pick_up(&mut s, PowerUpKind::SmallBomb);
    for _ in 0..200 {
        step(&mut s, &idle());
    }
    assert!(s.players[0].malus.is_none());
    let mut input = idle();
    input[0].bomb = true;
    step(&mut s, &input);
    assert_eq!(s.bombs.last().unwrap().range, 4);
}

#[test]
fn a_new_malus_replaces_the_old_one_and_the_same_one_restarts_the_timer() {
    let mut s = solo(1, 3);
    pick_up(&mut s, PowerUpKind::Slow);
    for _ in 0..50 {
        step(&mut s, &idle());
    }
    assert_eq!(s.players[0].malus.unwrap().ticks_left, 150);
    let events = pick_up(&mut s, PowerUpKind::HyperSpeed);
    let m = s.players[0].malus.unwrap();
    assert_eq!((m.kind, m.ticks_left), (PowerUpKind::HyperSpeed, 200));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MalusStarted {
            kind: PowerUpKind::HyperSpeed,
            ..
        }
    )));
    for _ in 0..30 {
        step(&mut s, &idle());
    }
    pick_up(&mut s, PowerUpKind::HyperSpeed);
    assert_eq!(s.players[0].malus.unwrap().ticks_left, 200, "restarted");
}

#[test]
fn bonuses_survive_a_malus_and_a_malus_does_not_touch_them() {
    let mut s = solo(1, 3);
    pick_up(&mut s, PowerUpKind::Punch);
    pick_up(&mut s, PowerUpKind::Speed);
    pick_up(&mut s, PowerUpKind::Slow);
    pick_up(&mut s, PowerUpKind::HyperSpeed);
    for _ in 0..200 {
        step(&mut s, &idle());
    }
    assert!(s.players[0].has_punch);
    assert_eq!(s.players[0].move_cooldown, 3);
    assert!(s.players[0].malus.is_none());
}

#[test]
fn elimination_clears_the_malus() {
    let mut s = solo(3, 3);
    pick_up(&mut s, PowerUpKind::Slow);
    let at = s.players[0].pos;
    s.flames.push(Flame {
        pos: at,
        ticks_left: 5,
    });
    step(&mut s, &idle());
    assert!(!s.players[0].alive);
    assert!(s.players[0].malus.is_none());
}

#[test]
fn a_new_round_starts_without_powers_or_maluses() {
    let s = solo(1, 3);
    assert!(!s.players[0].has_punch && s.players[0].malus.is_none());
}

#[test]
fn the_malus_clock_stops_with_the_round() {
    let mut s = state(&CORRIDOR, [Some((1, 3)), Some((5, 1)), None, None]);
    pick_up(&mut s, PowerUpKind::Slow);
    s.players[1].alive = false;
    step(&mut s, &idle());
    assert!(matches!(s.phase, bomberust_core::Phase::Ended(_)));
    let left = s.players[0].malus.unwrap().ticks_left;
    step(&mut s, &idle());
    assert_eq!(
        s.players[0].malus.unwrap().ticks_left,
        left,
        "an ended round does nothing"
    );
}
