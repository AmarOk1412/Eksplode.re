#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::kick::predict_slide_stop;
use eksplode_re_core::{
    step, Arena, Config, Dir, Event, Flight, GameState, PlayerInput, Pos, PowerUp, PowerUpKind,
    Slide,
};

fn p(x: u8, y: u8) -> Pos {
    Pos::new(x, y)
}

/// A 13x7 arena; `extra` sets tiles: (x, y, '#' wall | '+' block).
fn arena(extra: &[(u8, u8, char)]) -> Arena {
    let mut g: Vec<Vec<char>> = (0..7)
        .map(|y| {
            if y == 0 || y == 6 {
                "#".repeat(13).chars().collect()
            } else {
                format!("#{}#", ".".repeat(11)).chars().collect()
            }
        })
        .collect();
    for (x, y, c) in extra {
        g[usize::from(*y)][usize::from(*x)] = *c;
    }
    let rows: Vec<String> = g.into_iter().map(|r| r.into_iter().collect()).collect();
    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
    Arena::from_rows(&refs).unwrap()
}

/// Player 0 at (3, 3) with the kick and a bomb at (4, 3); `extra` tiles, other players and bombs.
fn setup(tiles: &[(u8, u8, char)], others: &[(usize, (u8, u8))], bombs: &[(u8, u8)]) -> GameState {
    let mut starts = [None; 4];
    starts[0] = Some(p(3, 3));
    for (slot, at) in others {
        starts[*slot] = Some(p(at.0, at.1));
    }
    let mut s = GameState::custom(Config::default(), arena(tiles), starts, 1).unwrap();
    s.players[0].has_kick = true;
    add_bomb(&mut s, 0, 4, 3, 1000);
    for at in bombs {
        add_bomb(&mut s, 1, at.0, at.1, 1000);
    }
    s
}

fn walk_right() -> [PlayerInput; 4] {
    let mut i = idle();
    i[0].mv = Some(Dir::Right);
    i
}

/// Kicks at once, then waits until the kicked bomb stops; returns where it rests and the events.
fn kick(mut s: GameState) -> (Pos, Vec<Event>) {
    let mut all = step(&mut s, &walk_right());
    for _ in 0..120 {
        if s.bombs[0].slide.is_none() {
            break;
        }
        all.extend(step(&mut s, &idle()));
    }
    assert!(s.bombs[0].slide.is_none(), "the bomb never stopped");
    (s.bombs[0].pos, all)
}

#[test]
fn f3_us2_as10_a_kicked_bomb_slides_to_the_wall() {
    let (at, events) = kick(setup(&[], &[], &[]));
    assert_eq!(at, p(11, 3));
    assert!(events.contains(&Event::BombStopped { pos: p(11, 3) }));
}

#[test]
fn f3_us2_as10_it_stops_before_a_block_a_bomb_or_a_living_player() {
    assert_eq!(kick(setup(&[(8, 3, '+')], &[], &[])).0, p(7, 3));
    assert_eq!(kick(setup(&[], &[], &[(9, 3)])).0, p(8, 3));
    assert_eq!(kick(setup(&[], &[(1, (7, 3))], &[])).0, p(6, 3));
    assert_eq!(kick(setup(&[(8, 3, '#')], &[], &[])).0, p(7, 3));
}

#[test]
fn f3_us2_as10_a_dead_player_does_not_block_the_slide() {
    // a third living player keeps the round going
    let mut s = setup(&[], &[(1, (7, 3)), (2, (1, 5))], &[]);
    s.players[1].alive = false;
    assert_eq!(kick(s).0, p(11, 3));
}

#[test]
fn f3_us2_as10_with_no_room_behind_the_bomb_nothing_moves() {
    for s in [
        setup(&[(5, 3, '+')], &[], &[]),
        setup(&[(5, 3, '#')], &[], &[]),
        setup(&[], &[(1, (5, 3))], &[]),
        setup(&[], &[], &[(5, 3)]),
    ] {
        let mut s = s;
        let events = step(&mut s, &walk_right());
        assert!(s.bombs[0].slide.is_none());
        assert_eq!(s.bombs[0].pos, p(4, 3));
        assert_eq!(
            s.players[0].pos,
            p(3, 3),
            "the bomb still blocks the player"
        );
        assert!(!events.iter().any(|e| matches!(e, Event::BombKicked { .. })));
    }
}

#[test]
fn f3_us2_as11_without_the_kick_the_bomb_just_blocks() {
    let mut s = setup(&[], &[], &[]);
    s.players[0].has_kick = false;
    let events = step(&mut s, &walk_right());
    assert!(s.bombs[0].slide.is_none());
    assert_eq!(s.bombs[0].pos, p(4, 3));
    assert!(events.is_empty() || !events.iter().any(|e| matches!(e, Event::BombKicked { .. })));
}

#[test]
fn f3_us2_as11_a_bomb_in_flight_cannot_be_kicked() {
    let mut s = setup(&[], &[], &[]);
    s.bombs[0].flight = Some(Flight {
        dir: Dir::Down,
        hop_left: 50,
        origin: p(4, 3),
    });
    step(&mut s, &walk_right());
    assert!(s.bombs[0].slide.is_none());
}

#[test]
fn f3_us2_as10_the_slide_moves_one_square_per_slide_ticks_and_emits_events() {
    let mut s = setup(&[], &[], &[]);
    let every = s.config.slide_ticks;
    assert_eq!(every, 3);
    let mut kicked_at = None;
    let mut slid = Vec::new();
    for t in 0..40u32 {
        let input = if t == 0 { walk_right() } else { idle() };
        for e in step(&mut s, &input) {
            match e {
                Event::BombKicked { player, from, dir } => {
                    assert_eq!((player, from, dir), (pid(0), p(4, 3), Dir::Right));
                    kicked_at = Some(t);
                }
                Event::BombSlid { pos } => slid.push((t, pos)),
                _ => {}
            }
        }
    }
    assert_eq!(kicked_at, Some(0));
    assert_eq!(
        slid[0],
        (every, p(5, 3)),
        "the first step comes slide_ticks later"
    );
    for w in slid.windows(2) {
        assert_eq!(w[1].0 - w[0].0, every);
        assert_eq!(w[1].1.x, w[0].1.x + 1);
    }
    assert_eq!(slid.last().unwrap().1, p(11, 3));
}

#[test]
fn f3_us2_as10_a_sliding_bomb_blocks_movement_and_keeps_its_fuse() {
    let mut s = setup(&[], &[], &[]);
    s.bombs[0].ticks_left = 40;
    step(&mut s, &walk_right());
    let before = s.bombs[0].ticks_left;
    let mut inputs = idle();
    inputs[0].mv = Some(Dir::Right);
    step(&mut s, &inputs);
    assert!(s.bombs[0].ticks_left < before, "the fuse keeps burning");
    assert!(s.bombs[0].slide.is_some());
    // walking after the bomb never overtakes it
    for _ in 0..30 {
        step(&mut s, &inputs);
        if s.bombs.is_empty() {
            break;
        }
        assert!(s.players[0].pos.x < s.bombs[0].pos.x);
    }
}

#[test]
fn f3_us2_as10_a_bomb_that_runs_out_of_fuse_while_sliding_explodes_where_it_is() {
    let mut s = setup(&[], &[], &[]);
    s.bombs[0].ticks_left = 8;
    let mut all = step(&mut s, &walk_right());
    for _ in 0..10 {
        all.extend(step(&mut s, &idle()));
    }
    let boom = all
        .iter()
        .find_map(|e| match e {
            Event::BombExploded { pos, .. } => Some(*pos),
            _ => None,
        })
        .expect("an explosion");
    assert!(
        boom.x > 4 && boom.x < 11,
        "exploded at {boom:?} in the middle of the slide"
    );
}

#[test]
fn f3_us2_as10_a_player_stepping_into_the_path_stops_the_bomb_before_them() {
    let mut s = setup(&[], &[(1, (8, 2))], &[]);
    step(&mut s, &walk_right());
    let mut down = idle();
    down[1].mv = Some(Dir::Down);
    step(&mut s, &down);
    for _ in 0..60 {
        step(&mut s, &idle());
    }
    assert_eq!(s.players[1].pos, p(8, 3));
    assert_eq!(s.bombs[0].pos, p(7, 3));
    assert!(s.bombs[0].slide.is_none());
}

#[test]
fn f3_us2_as10_the_prediction_matches_where_the_bomb_ends() {
    for s in [
        setup(&[], &[], &[]),
        setup(&[(8, 3, '+')], &[], &[]),
        setup(&[], &[(1, (7, 3))], &[]),
        setup(&[], &[], &[(9, 3)]),
    ] {
        let mut sim = s.clone();
        step(&mut sim, &walk_right());
        let predicted = predict_slide_stop(&sim.arena, &sim.players, &sim.bombs, 0);
        let (end, _) = kick(s);
        assert_eq!(predicted, end);
    }
    // a bomb that does not slide stays where it is
    let s = setup(&[], &[], &[]);
    assert_eq!(
        predict_slide_stop(&s.arena, &s.players, &s.bombs, 0),
        p(4, 3)
    );
}

#[test]
fn f3_us2_as10_a_punch_on_a_sliding_bomb_turns_it_into_a_flight() {
    let mut s = setup(&[], &[], &[]);
    s.players[0].has_punch = true;
    s.players[0].facing = Dir::Right;
    s.bombs[0].slide = Some(Slide {
        dir: Dir::Right,
        step_left: 3,
    });
    let mut i = idle();
    i[0].punch = true;
    step(&mut s, &i);
    assert!(s.bombs[0].slide.is_none());
    assert!(s.bombs[0].flight.is_some());
}

#[test]
fn f3_us2_as10_a_slide_in_other_directions_works_too() {
    for (dir, from, bomb, end) in [
        (Dir::Up, p(4, 5), p(4, 4), p(4, 1)),
        (Dir::Down, p(4, 1), p(4, 2), p(4, 5)),
        (Dir::Left, p(6, 3), p(5, 3), p(1, 3)),
    ] {
        let mut starts = [None; 4];
        starts[0] = Some(from);
        let mut s = GameState::custom(Config::default(), arena(&[]), starts, 1).unwrap();
        s.players[0].has_kick = true;
        add_bomb(&mut s, 0, bomb.x, bomb.y, 1000);
        let mut i = idle();
        i[0].mv = Some(dir);
        step(&mut s, &i);
        for _ in 0..60 {
            step(&mut s, &idle());
        }
        assert_eq!(s.bombs[0].pos, end, "{dir:?}");
    }
}

#[test]
fn f3_us2_as10_picking_up_the_kick_sets_has_kick_and_a_new_round_starts_without_it() {
    let mut starts = [None; 4];
    starts[0] = Some(p(3, 3));
    let mut s = GameState::custom(Config::default(), arena(&[]), starts, 1).unwrap();
    s.powerups.push(PowerUp {
        pos: p(4, 3),
        kind: PowerUpKind::Kick,
    });
    assert!(!s.players[0].has_kick);
    let events = step(&mut s, &walk_right());
    assert!(s.players[0].has_kick);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::PowerUpCollected {
            kind: PowerUpKind::Kick,
            ..
        }
    )));
    assert!(!PowerUpKind::Kick.is_malus());
    assert_eq!(PowerUpKind::ALL.last(), Some(&PowerUpKind::Kick));
    let fresh = GameState::new(Config::default(), 3, [true; 4]).unwrap();
    assert!(fresh.players.iter().all(|pl| !pl.has_kick));
}

#[test]
fn f3_us2_as10_the_same_inputs_give_the_same_slide() {
    let run = || {
        let mut s = setup(&[(9, 3, '+')], &[], &[]);
        let mut all = step(&mut s, &walk_right());
        for _ in 0..40 {
            all.extend(step(&mut s, &idle()));
        }
        (s, all)
    };
    assert_eq!(run(), run());
}

#[test]
fn f3_us2_as12_the_explosion_prediction_matches_where_a_sliding_bomb_goes_off() {
    for fuse in [3u32, 5, 8, 11, 14, 40] {
        let mut s = setup(&[], &[], &[]);
        s.bombs[0].ticks_left = fuse;
        let mut all = step(&mut s, &walk_right());
        let predicted = eksplode_re_core::kick::predict_slide_explosion(
            &s.arena,
            &s.players,
            &s.bombs,
            0,
            s.config.slide_ticks,
        );
        for _ in 0..fuse + 2 {
            all.extend(step(&mut s, &idle()));
        }
        let boom = all
            .iter()
            .find_map(|e| match e {
                Event::BombExploded { pos, .. } => Some(*pos),
                _ => None,
            })
            .expect("an explosion");
        assert_eq!(boom, predicted, "fuse {fuse}");
    }
}
