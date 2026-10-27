#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::closing::{closing_order, interval};
use eksplode_re_core::{
    step, Config, Dir, Event, Falling, Flame, GameState, Phase, PlayerInput, PowerUp, PowerUpKind,
    Rng, Tile,
};

/// A 7x7 open arena whose pattern starts at tick 50, announces a square every 2 ticks (25 squares,
/// 50 ticks) and lands it 5 ticks later.
fn config() -> Config {
    Config {
        round_limit_ticks: 100,
        closing_ticks: 50,
        closing_lead_ticks: 5,
        ..Config::default()
    }
}

fn open_state(starts: [Option<(u8, u8)>; 4]) -> GameState {
    state_with(config(), &OPEN, starts)
}

/// Steps with idle inputs until `s.tick == tick`.
fn run_to(s: &mut GameState, tick: u32) {
    while s.tick < tick {
        step(s, &idle());
    }
}

fn warned(ev: &[Event]) -> Vec<(u8, u8)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::BlockWarned { pos } => Some((pos.x, pos.y)),
            _ => None,
        })
        .collect()
}

fn landed(ev: &[Event]) -> Vec<(u8, u8)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::BlockLanded { pos } => Some((pos.x, pos.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_order_is_a_clockwise_spiral_from_the_top_left() {
    let order: Vec<(u8, u8)> = closing_order(7, 7).iter().map(|p| (p.x, p.y)).collect();
    let expected = [
        // outer ring: top row, right column, bottom row, left column
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (5, 2),
        (5, 3),
        (5, 4),
        (5, 5),
        (4, 5),
        (3, 5),
        (2, 5),
        (1, 5),
        (1, 4),
        (1, 3),
        (1, 2),
        // next ring
        (2, 2),
        (3, 2),
        (4, 2),
        (4, 3),
        (4, 4),
        (3, 4),
        (2, 4),
        (2, 3),
        // the middle
        (3, 3),
    ];
    assert_eq!(order, expected);

    // every inner square exactly once, on any size
    for (w, h) in [(15, 13), (19, 15), (25, 19), (9, 7)] {
        let order = closing_order(w, h);
        assert_eq!(
            order.len(),
            usize::from(w - 2) * usize::from(h - 2),
            "{w}x{h}"
        );
        let mut seen = std::collections::BTreeSet::new();
        for p in &order {
            assert!(p.x >= 1 && p.x < w - 1 && p.y >= 1 && p.y < h - 1);
            assert!(seen.insert(*p), "{p:?} twice");
        }
        assert_eq!((order[0].x, order[0].y), (1, 1), "starts at the outer ring");
    }
}

#[test]
fn f4_us2_as1_the_pattern_starts_60_seconds_before_the_end_from_the_outer_ring() {
    let mut s = open_state([Some((3, 3)), Some((3, 4)), None, None]);
    assert_eq!(config().round_limit_ticks - config().closing_ticks, 50);
    while s.tick < 49 {
        let ev = step(&mut s, &idle());
        assert!(warned(&ev).is_empty() && !ev.contains(&Event::PatternStarted));
    }
    assert!(s.closing.falling.is_empty());
    assert!(!s.closing.started);

    // tick 49 -> 50: nothing yet; the step at tick 50 starts the pattern
    assert_eq!(s.tick, 49);
    step(&mut s, &idle());
    assert_eq!(s.tick, 50);
    let ev = step(&mut s, &idle());
    assert_eq!(
        ev.iter().filter(|e| **e == Event::PatternStarted).count(),
        1
    );
    assert_eq!(
        warned(&ev),
        vec![(1, 1)],
        "the first square is in the outer ring"
    );
    assert_eq!(
        s.closing.falling,
        vec![Falling {
            pos: pos(1, 1),
            ticks_left: 5
        }]
    );
    assert_eq!(
        s.observe(pid(0)).falling,
        s.closing.falling,
        "clients see the announced squares"
    );

    // one more every 2 ticks (the interval), counting down
    let ev = step(&mut s, &idle());
    assert!(warned(&ev).is_empty());
    assert_eq!(s.closing.falling[0].ticks_left, 4);
    let ev = step(&mut s, &idle());
    assert_eq!(warned(&ev), vec![(2, 1)]);
    assert_eq!(s.closing.falling.len(), 2);
    assert!(
        !ev.contains(&Event::PatternStarted),
        "the start is announced once"
    );
}

#[test]
fn a_block_lands_after_the_countdown_and_becomes_a_wall() {
    let mut s = open_state([Some((3, 3)), Some((3, 4)), None, None]);
    run_to(&mut s, 50);
    let mut landed_at = None;
    for _ in 0..10 {
        let t = s.tick;
        let ev = step(&mut s, &idle());
        if landed(&ev) == vec![(1, 1)] {
            landed_at = Some(t);
            break;
        }
    }
    assert_eq!(landed_at, Some(55), "announced at 50, lands 5 ticks later");
    assert_eq!(s.arena.tile(pos(1, 1)), Tile::Wall);
    assert!(s.closing.falling.iter().all(|f| f.pos != pos(1, 1)));
}

#[test]
fn the_interval_covers_the_board_in_the_pattern_time_on_every_size() {
    for (w, h) in [(15u8, 13u8), (19, 15), (25, 19)] {
        let c = Config {
            width: w,
            height: h,
            ..Config::default()
        };
        let mut rng = Rng::new(7);
        let arena = eksplode_re_core::Arena::generate(&c, &mut rng);
        let n = closing_order(w, h)
            .iter()
            .filter(|p| arena.tile(**p) != Tile::Wall)
            .count() as u32;
        let every = interval(c.closing_ticks, n);
        assert!(every >= 1);
        // the last square is announced inside the pattern time and lands one lead later
        let last_announce = (n - 1) * every;
        assert!(last_announce < c.closing_ticks, "{w}x{h}: {last_announce}");
        assert!(last_announce + c.closing_lead_ticks <= c.closing_ticks + c.closing_lead_ticks);
        // and the pace is not slower than needed: the whole thing takes more than half the time
        assert!(n * every * 2 > c.closing_ticks || every == 1, "{w}x{h}");
    }
    assert_eq!(interval(1200, 143), 8);
    assert_eq!(interval(10, 400), 1, "never less than one tick");
    assert_eq!(interval(1200, 0), 1, "no squares, no division by zero");
}

#[test]
fn the_whole_board_is_covered_within_the_pattern_time_plus_the_lead() {
    for (w, h) in [(15u8, 13u8), (19, 15), (25, 19)] {
        let c = Config {
            width: w,
            height: h,
            round_limit_ticks: 3600,
            ..Config::default()
        };
        let mut s = GameState::new(c.clone(), 3, [true; 4]).unwrap();
        // run the pattern alone: nobody can die, so call it directly
        let t0 = c.round_limit_ticks - c.closing_ticks;
        let before: Vec<_> = s
            .arena
            .positions_of(Tile::Floor)
            .into_iter()
            .chain(s.arena.positions_of(Tile::Block))
            .filter(|p| p.x >= 1 && p.x < w - 1 && p.y >= 1 && p.y < h - 1)
            .collect();
        let mut ev = Vec::new();
        s.tick = t0;
        while s.tick < c.round_limit_ticks + c.closing_lead_ticks {
            eksplode_re_core::closing::advance(&mut s, &mut ev);
            s.tick += 1;
        }
        for p in before {
            assert_eq!(
                s.arena.tile(p),
                Tile::Wall,
                "{p:?} on {w}x{h} is not covered"
            );
        }
        assert!(s.closing.falling.is_empty());
    }
}

#[test]
fn walls_are_skipped_without_using_an_interval() {
    // a pillar in the middle of the first row
    let mut rows: Vec<String> = OPEN.iter().map(|r| r.to_string()).collect();
    rows[1] = "#..#..#".to_string();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut s = state_with(config(), &rows, [Some((3, 3)), Some((3, 4)), None, None]);
    run_to(&mut s, 50);
    let mut seen = Vec::new();
    while s.tick < 60 {
        let ev = step(&mut s, &idle());
        seen.extend(warned(&ev));
    }
    // 24 free squares: interval 50 / 24 = 2; (3,1) is a wall and does not delay (4,1)
    assert_eq!(&seen[..4], &[(1, 1), (2, 1), (4, 1), (5, 1)]);
}

#[test]
fn f4_us2_as2_a_landing_block_crushes_a_player_and_destroys_bombs_and_powerups() {
    // squares land in order (1,1) t55, (2,1) t57, (3,1) t59, (4,1) t61, (5,1) t63
    let mut s = open_state([Some((3, 1)), Some((5, 5)), Some((3, 5)), None]);
    run_to(&mut s, 50);
    add_bomb(&mut s, 1, 1, 1, 500);
    s.powerups.push(PowerUp {
        pos: pos(2, 1),
        kind: PowerUpKind::Speed,
    });
    s.flames.push(Flame {
        pos: pos(4, 1),
        ticks_left: 100,
        owners: 0,
    });

    let mut all = Vec::new();
    while s.tick < 64 {
        all.extend(step(&mut s, &idle()));
    }
    // the bomb vanished without exploding and its owner can place bombs again
    assert!(s.bombs.is_empty());
    assert_eq!(s.players[1].bombs_active, 0);
    assert!(!all.iter().any(|e| matches!(e, Event::BombExploded { .. })));
    // the power-up was destroyed
    assert!(s.powerups.is_empty());
    assert!(all.contains(&Event::PowerUpDestroyed { pos: pos(2, 1) }));
    // the player on the third square was eliminated in the tick it landed
    assert!(all.contains(&Event::PlayerEliminated { player: pid(0) }));
    assert!(!s.players[0].alive);
    // the flame on a landing square is gone
    assert!(s.flames.iter().all(|f| f.pos != pos(4, 1)));
    // all five squares are walls explosions cannot remove
    for x in 1..=5 {
        assert_eq!(s.arena.tile(pos(x, 1)), Tile::Wall, "x={x}");
    }
    assert_eq!(landed(&all), vec![(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
}

#[test]
fn the_player_dies_in_the_tick_of_the_landing_not_before() {
    let mut s = open_state([Some((3, 1)), Some((5, 5)), None, None]);
    run_to(&mut s, 50);
    while s.tick < 59 {
        step(&mut s, &idle());
        assert!(s.players[0].alive, "tick {}", s.tick);
    }
    let ev = step(&mut s, &idle());
    assert_eq!(landed(&ev), vec![(3, 1)]);
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(0) }));
}

#[test]
fn an_announced_square_can_still_be_walked_on_and_the_walker_is_crushed() {
    let mut s = open_state([Some((2, 2)), Some((5, 5)), Some((3, 5)), None]);
    run_to(&mut s, 51);
    // (1,1) is announced since tick 50; step onto it
    let left = PlayerInput {
        mv: Some(Dir::Left),
        ..PlayerInput::default()
    };
    let up = PlayerInput {
        mv: Some(Dir::Up),
        ..PlayerInput::default()
    };
    let mut inputs = idle();
    inputs[0] = left;
    step(&mut s, &inputs);
    s.players[0].cooldown_left = 0;
    inputs[0] = up;
    step(&mut s, &inputs);
    assert_eq!(
        s.players[0].pos,
        pos(1, 1),
        "an announced square is walkable"
    );
    let mut crushed = false;
    while s.tick < 60 {
        let ev = step(&mut s, &idle());
        crushed |= ev.contains(&Event::PlayerEliminated { player: pid(0) });
    }
    assert!(crushed);
}

#[test]
fn a_bomb_that_explodes_in_the_tick_a_block_lands_still_explodes() {
    let mut s = open_state([Some((5, 5)), Some((3, 5)), None, None]);
    run_to(&mut s, 55);
    // the first square lands during the step at tick 55; the fuse also ends then
    add_bomb(&mut s, 0, 1, 1, 1);
    let ev = step(&mut s, &idle());
    assert_eq!(s.tick, 56);
    assert!(ev.contains(&Event::BombExploded {
        owner: pid(0),
        pos: pos(1, 1)
    }));
    assert_eq!(landed(&ev), vec![(1, 1)]);
    assert_eq!(s.arena.tile(pos(1, 1)), Tile::Wall);
    assert_eq!(s.players[0].bombs_active, 0);
}

#[test]
fn a_flying_bomb_is_unaffected_until_it_lands() {
    let mut s = open_state([Some((5, 5)), Some((3, 5)), None, None]);
    run_to(&mut s, 55);
    add_bomb(&mut s, 0, 1, 1, 500);
    s.bombs[0].flight = Some(eksplode_re_core::Flight {
        dir: Dir::Down,
        hop_left: 1000,
        origin: pos(1, 1),
    });
    let ev = step(&mut s, &idle());
    assert_eq!(landed(&ev), vec![(1, 1)]);
    assert_eq!(s.bombs.len(), 1, "the bomb in the air stays");
    assert_eq!(s.players[0].bombs_active, 1);
}

#[test]
fn a_kicked_bomb_that_stops_on_an_announced_square_is_destroyed_on_landing() {
    let mut s = open_state([Some((5, 5)), Some((3, 5)), None, None]);
    run_to(&mut s, 54);
    add_bomb(&mut s, 0, 1, 1, 500);
    s.bombs[0].slide = None;
    let ev = step(&mut s, &idle());
    let ev2 = step(&mut s, &idle());
    let all: Vec<_> = ev.into_iter().chain(ev2).collect();
    assert_eq!(landed(&all), vec![(1, 1)]);
    assert!(s.bombs.is_empty());
    assert!(!all.iter().any(|e| matches!(e, Event::BombExploded { .. })));
}

#[test]
fn the_pattern_draws_no_random_numbers() {
    let mut a = open_state([Some((3, 3)), Some((3, 4)), None, None]);
    let rng_before = a.rng.clone();
    run_to(&mut a, 80);
    assert_eq!(
        a.rng, rng_before,
        "the state RNG is untouched by the pattern"
    );
    assert!(a.phase == Phase::Playing || matches!(a.phase, Phase::Ended(_)));
}
