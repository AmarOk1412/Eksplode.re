#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{
    step, Arena, BoardSize, Config, DropTable, Event, Flame, GameState, Mode, Outcome, Phase,
    PlayerInput, PowerUpKind, Rng, Role, RoomSettings, Tile,
};

fn chaser_config() -> Config {
    Config {
        mode: Mode::Chaser,
        ..Config::default()
    }
}

fn sized(size: BoardSize) -> Config {
    RoomSettings {
        size,
        mode: Mode::Chaser,
        ..RoomSettings::default()
    }
    .to_config(&Config::default())
}

/// The first participant is the chaser on a custom arena: slot 0 in the middle of the 7x7 arena,
/// the runners in three corners.
fn duel_state(config: Config) -> GameState {
    let starts = [Some((3, 3)), Some((1, 1)), Some((5, 1)), Some((1, 5))];
    state_with(config, &OPEN, starts)
}

fn flame(s: &mut GameState, x: u8, y: u8) {
    s.flames.push(Flame {
        pos: pos(x, y),
        ticks_left: 3,
        owners: 0,
    });
}

fn alive(s: &GameState) -> [bool; 4] {
    [0, 1, 2, 3].map(|i| s.players[i].alive)
}

#[test]
fn f4_us4_as1_one_chaser_in_the_center_with_two_bombs_and_three_hearts_and_runners_in_the_corners()
{
    let mut chasers = std::collections::BTreeSet::new();
    for size in BoardSize::ALL {
        let config = sized(size);
        let (w, h) = (config.width, config.height);
        for seed in 0..40u64 {
            let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
            let chaser: Vec<usize> = (0..4)
                .filter(|i| s.players[*i].role == Role::Chaser)
                .collect();
            assert_eq!(chaser.len(), 1, "{size:?} seed {seed}");
            let c = chaser[0];
            chasers.insert(c);
            assert_eq!(
                s.players[c].pos,
                pos(w / 2, h / 2),
                "the chaser starts in the middle"
            );
            assert_eq!(s.players[c].bombs_max, 2);
            assert_eq!(s.players[c].hearts, 3);
            let corners = [pos(1, 1), pos(w - 2, 1), pos(1, h - 2), pos(w - 2, h - 2)];
            for i in (0..4).filter(|i| *i != c) {
                assert_eq!(s.players[i].role, Role::Runner);
                assert_eq!(s.players[i].pos, corners[i], "runners keep their corners");
                assert_eq!(s.players[i].bombs_max, 0, "runners have no bombs");
                assert_eq!(s.players[i].hearts, 1);
            }
            assert!(
                s.players.iter().all(|p| p.pos != corners[c]),
                "the chaser's own corner stays empty"
            );
            // the first step tells everybody the roles
            let ev = step(&mut s, &idle());
            let assigned = ev
                .iter()
                .filter(|e| matches!(e, Event::RoleAssigned { .. }))
                .count();
            assert_eq!(assigned, 4);
            assert!(ev.contains(&Event::RoleAssigned {
                player: pid(c as u8),
                role: Role::Chaser
            }));
        }
    }
    assert!(
        chasers.len() >= 3,
        "the role is drawn from the seed: {chasers:?}"
    );
}

#[test]
fn the_chaser_is_drawn_after_the_arena_so_the_arena_of_a_seed_does_not_depend_on_it() {
    let config = chaser_config();
    for seed in 0..30u64 {
        let s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let arena = Arena::generate(&config, &mut Rng::new(seed));
        assert_eq!(s.arena, arena, "seed {seed}");
    }
    // and the draw is reproducible
    let a = GameState::new(config.clone(), 9, [true; 4]).unwrap();
    let b = GameState::new(config, 9, [true; 4]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn the_chaser_arena_has_fewer_pillars_and_blocks_and_is_fully_reachable() {
    for size in BoardSize::ALL {
        let config = sized(size);
        let (w, h) = (config.width, config.height);
        let normal = Config {
            mode: Mode::Normal,
            ..config.clone()
        };
        let count = |a: &Arena, t: Tile| a.positions_of(t).len();
        let mut blocks = 0usize;
        let mut free = 0usize;
        for seed in 0..40u64 {
            let s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
            let n = GameState::new(normal.clone(), seed, [true; 4]).unwrap();
            let inner_pillars = |a: &Arena| {
                (2..h - 1)
                    .step_by(2)
                    .flat_map(|y| (2..w - 1).step_by(2).map(move |x| pos(x, y)))
                    .filter(|p| a.tile(*p) == Tile::Wall)
                    .count()
            };
            let (cp, np) = (inner_pillars(&s.arena), inner_pillars(&n.arena));
            assert!(
                cp * 2 < np + 4,
                "{size:?}: {cp} pillars against {np} in Normal mode"
            );
            for y in 0..h {
                for x in 0..w {
                    let p = pos(x, y);
                    let border = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                    let pillar = x % 2 == 0 && y % 2 == 0 && !border;
                    if pillar {
                        let keeps = (usize::from(x / 2) + usize::from(y / 2)) % 3 == 0;
                        let near_centre = x.abs_diff(w / 2) + y.abs_diff(h / 2) <= 1;
                        if near_centre {
                            assert_eq!(s.arena.tile(p), Tile::Floor, "the middle cross is clear");
                        } else {
                            assert_eq!(
                                s.arena.tile(p) == Tile::Wall,
                                keeps,
                                "{size:?} pillar {x},{y}"
                            );
                        }
                    }
                }
            }
            // the middle cross and the corners are clear, everything can be reached
            let centre = pos(w / 2, h / 2);
            for p in [
                centre,
                pos(centre.x + 1, centre.y),
                pos(centre.x - 1, centre.y),
                pos(centre.x, centre.y + 1),
                pos(centre.x, centre.y - 1),
            ] {
                assert_eq!(s.arena.tile(p), Tile::Floor, "{size:?} seed {seed} {p:?}");
            }
            let non_wall = count(&s.arena, Tile::Floor) + count(&s.arena, Tile::Block);
            let mut seen = std::collections::BTreeSet::from([centre]);
            let mut queue = std::collections::VecDeque::from([centre]);
            while let Some(p) = queue.pop_front() {
                for d in eksplode_re_core::Dir::ALL {
                    if let Some(n) = p.step(d, w, h) {
                        if s.arena.tile(n) != Tile::Wall && seen.insert(n) {
                            queue.push_back(n);
                        }
                    }
                }
            }
            assert_eq!(
                seen.len(),
                non_wall,
                "{size:?} seed {seed}: every square is reachable"
            );
            blocks += count(&s.arena, Tile::Block);
            free += non_wall;
        }
        let share = blocks as f32 / free as f32;
        assert!(
            (0.05..=0.2).contains(&share),
            "{size:?}: block share {share}"
        );
    }
}

#[test]
fn f4_us4_as2_runners_cannot_place_bombs_and_extra_bomb_and_flame_never_appear() {
    let mut s = duel_state(chaser_config());
    let mut inputs = idle();
    for input in inputs.iter_mut().skip(1) {
        *input = bomb_input();
    }
    let mut placed = 0;
    for _ in 0..20 {
        placed += step(&mut s, &inputs)
            .iter()
            .filter(|e| matches!(e, Event::BombPlaced { .. }))
            .count();
    }
    assert_eq!(placed, 0, "no runner can place a bomb");
    assert!(s.bombs.is_empty());

    // the chaser can
    let mut inputs = idle();
    inputs[0] = bomb_input();
    let ev = step(&mut s, &inputs);
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BombPlaced { player, .. } if *player == pid(0))));

    // no bonus that gives bombs or range is ever offered
    let config = Config {
        bonus_every_ticks: 1,
        bonus_cap: 200,
        ..chaser_config()
    };
    let mut s = state_with(config, &OPEN, [Some((1, 1)), Some((5, 5)), None, None]);
    let mut kinds = std::collections::BTreeSet::new();
    for _ in 0..3000 {
        for e in step(&mut s, &idle()) {
            if let Event::PowerUpSpawned { kind, .. } = e {
                kinds.insert(format!("{kind:?}"));
            }
        }
        s.powerups.clear();
    }
    assert!(
        !kinds.contains("ExtraBomb") && !kinds.contains("Range"),
        "{kinds:?}"
    );
    assert_eq!(kinds.len(), 6, "every allowed kind shows up: {kinds:?}");
}

#[test]
fn f4_us4_as3_bonuses_appear_at_a_regular_pace_on_free_squares_up_to_the_cap() {
    // pace: one every `bonus_every_ticks`, with the cap out of the way
    let config = Config {
        bonus_every_ticks: 100,
        bonus_cap: 200,
        ..chaser_config()
    };
    let mut s = state_with(config, &OPEN, [Some((1, 1)), Some((5, 5)), None, None]);
    let mut at = Vec::new();
    for _ in 0..1000 {
        let t = s.tick;
        for e in step(&mut s, &idle()) {
            if let Event::PowerUpSpawned { .. } = e {
                at.push(t);
            }
        }
        s.powerups.clear();
    }
    assert_eq!(at.len(), 10, "{at:?}");
    for pair in at.windows(2) {
        assert_eq!(pair[1] - pair[0], 100);
    }

    // squares: never on a wall, block, bomb, player or another bonus
    let config = Config {
        bonus_every_ticks: 1,
        bonus_cap: 200,
        ..chaser_config()
    };
    let rows = [
        "#######", "#.#...#", "#.+...#", "#.....#", "#...#.#", "#.....#", "#######",
    ];
    let mut s = state_with(config, &rows, [Some((1, 1)), Some((5, 5)), None, None]);
    add_bomb(&mut s, 0, 3, 3, 100_000);
    for _ in 0..400 {
        for e in step(&mut s, &idle()) {
            if let Event::PowerUpSpawned { pos: p, .. } = e {
                assert_eq!(s.arena.tile(p), Tile::Floor, "{p:?}");
                assert!(s.bombs.iter().all(|b| b.pos != p));
                assert!(s.players.iter().all(|pl| !pl.alive || pl.pos != p));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        assert!(
            s.powerups.iter().all(|u| seen.insert(u.pos)),
            "two bonuses on one square"
        );
    }
    assert!(!s.powerups.is_empty());

    // cap: never more than `bonus_cap` on the board
    let config = Config {
        bonus_every_ticks: 5,
        bonus_cap: 4,
        ..chaser_config()
    };
    let mut s = state_with(config, &OPEN, [Some((1, 1)), Some((5, 5)), None, None]);
    for _ in 0..500 {
        step(&mut s, &idle());
        assert!(s.powerups.len() <= 4);
    }
    assert_eq!(s.powerups.len(), 4, "the board fills up to the cap");
    // taking one away makes room for the next
    s.powerups.pop();
    for _ in 0..5 {
        step(&mut s, &idle());
    }
    assert_eq!(s.powerups.len(), 4);
}

#[test]
fn the_bonus_weights_follow_the_schedule_and_the_drop_table_has_no_effect() {
    let config = Config {
        bonus_every_ticks: 1,
        bonus_cap: 200,
        round_limit_ticks: 1_000_000,
        ..chaser_config()
    };
    let mut s = state_with(config, &OPEN, [Some((1, 1)), Some((5, 5)), None, None]);
    let mut counts = std::collections::BTreeMap::new();
    let total = 20_000;
    let mut n = 0;
    while n < total {
        for e in step(&mut s, &idle()) {
            if let Event::PowerUpSpawned { kind, .. } = e {
                *counts.entry(format!("{kind:?}")).or_insert(0usize) += 1;
                n += 1;
            }
        }
        s.powerups.clear();
    }
    // speed 3, kick 2, punch 2, slow 1, hyper speed 1, small bomb 1 (out of 10)
    for (kind, weight) in [
        ("Speed", 3.0),
        ("Kick", 2.0),
        ("Punch", 2.0),
        ("Slow", 1.0),
        ("HyperSpeed", 1.0),
        ("SmallBomb", 1.0),
    ] {
        let share = counts[kind] as f32 / total as f32;
        assert!(
            (share - weight / 10.0).abs() < 0.1 * weight / 10.0 + 0.01,
            "{kind}: {share}"
        );
    }

    // a destroyed block drops nothing in this mode, whatever the table says
    let rows = [
        "#######", "#..+..#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let config = Config {
        bonus_every_ticks: 100_000,
        ..chaser_config()
    };
    let mut s = state_with(
        config,
        &rows,
        [Some((3, 3)), Some((1, 5)), Some((5, 5)), None],
    );
    s.set_drops(DropTable::new([0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]).unwrap())
        .unwrap();
    add_bomb(&mut s, 0, 3, 2, 1);
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::BlockDestroyed { pos: pos(3, 1) }));
    assert!(!ev.iter().any(|e| matches!(e, Event::PowerUpSpawned { .. })));
    assert!(s.powerups.is_empty());
}

#[test]
fn f4_us4_as4_the_chaser_loses_a_heart_per_explosion_with_two_seconds_of_protection() {
    let mut s = duel_state(chaser_config());
    step(&mut s, &idle());
    flame(&mut s, 3, 3);
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::HeartLost {
        player: pid(0),
        hearts_left: 2
    }));
    assert!(s.players[0].alive);
    assert_eq!(s.players[0].hearts, 2);
    assert_eq!(s.players[0].shield_left, s.config.shield_ticks);

    // protected: more flames during the protection do not cost a heart
    for _ in 0..(s.config.shield_ticks - 1) {
        flame(&mut s, 3, 3);
        step(&mut s, &idle());
        assert_eq!(s.players[0].hearts, 2, "tick {}", s.tick);
    }
    // the protection ran out: the next flame costs the next heart
    s.flames.clear();
    for _ in 0..3 {
        step(&mut s, &idle());
    }
    assert_eq!(s.players[0].shield_left, 0);
    flame(&mut s, 3, 3);
    let ev = step(&mut s, &idle());
    assert!(ev.contains(&Event::HeartLost {
        player: pid(0),
        hearts_left: 1
    }));

    // the last heart eliminates the chaser and the runners win
    s.flames.clear();
    for _ in 0..(s.config.shield_ticks + 2) {
        step(&mut s, &idle());
    }
    flame(&mut s, 3, 3);
    let ev = step(&mut s, &idle());
    assert!(!s.players[0].alive);
    assert!(ev.contains(&Event::PlayerEliminated { player: pid(0) }));
    assert_eq!(s.phase, Phase::Ended(Outcome::RunnersWin));
}

#[test]
fn the_chasers_own_explosion_costs_a_heart_too() {
    let mut s = duel_state(chaser_config());
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert!(ev.iter().any(|e| matches!(e, Event::BombExploded { .. })));
    assert!(ev.contains(&Event::HeartLost {
        player: pid(0),
        hearts_left: 2
    }));
    assert!(s.players[0].alive);
}

#[test]
fn f4_us4_as5_a_runner_dies_from_one_explosion_and_the_chaser_wins_when_the_last_runner_dies() {
    let mut s = duel_state(chaser_config());
    flame(&mut s, 1, 1);
    step(&mut s, &idle());
    assert_eq!(
        alive(&s),
        [true, false, true, true],
        "one flame eliminates a runner"
    );
    assert_eq!(s.phase, Phase::Playing);
    flame(&mut s, 5, 1);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Playing);
    flame(&mut s, 1, 5);
    let ev = step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::ChaserWins));
    assert!(ev.contains(&Event::RoundEnded(Outcome::ChaserWins)));
    assert!(s.players[0].alive);
}

#[test]
fn f4_us4_as6_the_runners_win_when_the_time_runs_out_and_no_pattern_runs() {
    let config = Config {
        round_limit_ticks: 40,
        closing_ticks: 20,
        ..chaser_config()
    };
    let mut s = duel_state(config);
    let mut all = Vec::new();
    while s.phase == Phase::Playing {
        all.extend(step(&mut s, &idle()));
        assert!(s.tick <= 40);
    }
    assert_eq!(s.tick, 40);
    assert_eq!(s.phase, Phase::Ended(Outcome::RunnersWin));
    assert!(
        !all.contains(&Event::PatternStarted),
        "no end pattern in Chaser mode"
    );
    assert!(all
        .iter()
        .all(|e| !matches!(e, Event::BlockWarned { .. } | Event::BlockLanded { .. })));
    assert!(s.closing.falling.is_empty());
}

#[test]
fn the_runners_win_when_the_chaser_is_out_and_everybody_dead_is_a_draw() {
    let mut s = duel_state(chaser_config());
    s.players[0].hearts = 1;
    flame(&mut s, 3, 3);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::RunnersWin));

    let mut s = duel_state(chaser_config());
    s.players[0].hearts = 1;
    for (x, y) in [(3, 3), (1, 1), (5, 1), (1, 5)] {
        flame(&mut s, x, y);
    }
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
}

#[test]
fn f4_us4_as7_runners_use_kick_punch_and_speed_as_in_the_other_modes() {
    // a runner with the punch sends the chaser's bomb on its way
    let mut s = duel_state(chaser_config());
    s.players[3].pos = pos(1, 3);
    s.players[3].has_punch = true;
    s.players[3].facing = eksplode_re_core::Dir::Right;
    add_bomb(&mut s, 0, 2, 3, 100);
    let mut inputs = idle();
    inputs[3] = PlayerInput {
        punch: true,
        ..PlayerInput::default()
    };
    let ev = step(&mut s, &inputs);
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BombPunched { player, .. } if *player == pid(3))));

    // a runner with the kick pushes it
    let mut s = duel_state(chaser_config());
    s.players[0].pos = pos(5, 5); // out of the way of the sliding bomb
    s.players[3].pos = pos(1, 3);
    s.players[3].has_kick = true;
    add_bomb(&mut s, 0, 2, 3, 100);
    let mut inputs = idle();
    inputs[3] = PlayerInput {
        mv: Some(eksplode_re_core::Dir::Right),
        ..PlayerInput::default()
    };
    let ev = step(&mut s, &inputs);
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BombKicked { player, .. } if *player == pid(3))));

    // and a speed bonus makes the runner faster
    let mut s = duel_state(chaser_config());
    powerup_at(&mut s, 1, 2, PowerUpKind::Speed);
    s.players[1].pos = pos(1, 1);
    let before = s.players[1].move_cooldown;
    let mut inputs = idle();
    inputs[1] = PlayerInput {
        mv: Some(eksplode_re_core::Dir::Down),
        ..PlayerInput::default()
    };
    step(&mut s, &inputs);
    assert!(s.players[1].move_cooldown < before);
}

fn powerup_at(s: &mut GameState, x: u8, y: u8, kind: PowerUpKind) {
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(x, y),
        kind,
    });
}

#[test]
fn a_practice_round_in_chaser_mode_has_no_roles() {
    let s = state_with(chaser_config(), &OPEN, [Some((3, 3)), None, None, None]);
    assert_eq!(s.players[0].role, Role::Normal);
    assert_eq!(s.players[0].hearts, 1);
}
