mod common;

use common::*;
use eksplode_re_bots::{Bot, DefaultBot};
use eksplode_re_core::{step, Dir, Event, GameState, PlayerInput};

fn decide(s: &GameState) -> PlayerInput {
    DefaultBot::new(1).decide(&obs(s, 0))
}

/// Runs the bot as player 0 and everything else idle.
fn run_bot(s: &mut GameState, ticks: u32) -> Vec<Event> {
    let mut bot = DefaultBot::new(1);
    let mut all = Vec::new();
    for _ in 0..ticks {
        let mut inputs = [PlayerInput::default(); 4];
        inputs[0] = bot.decide(&s.observe(pid(0)));
        all.extend(step(s, &inputs));
    }
    all
}

#[test]
fn us2_as2_flees_blast_when_safe_path_exists() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 2, 20);
    run_bot(&mut s, 40);
    assert!(s.players[0].alive);
    assert!(s.flames.is_empty() || !s.flames.iter().any(|f| f.pos == s.players[0].pos));
}

#[test]
fn flees_from_a_bomb_on_its_own_tile() {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    add_bomb(&mut s, 0, 3, 3, 30);
    run_bot(&mut s, 45);
    assert!(s.players[0].alive);
}

#[test]
fn flees_chain_reactions() {
    let mut s = duel(&OPEN, (5, 5), (1, 1));
    add_bomb(&mut s, 1, 3, 3, 3);
    add_bomb(&mut s, 1, 5, 3, 60);
    s.players[1].bombs_max = 2;
    run_bot(&mut s, 25);
    assert!(
        s.players[0].alive,
        "second bomb explodes together with the first"
    );
}

#[test]
fn us2_as3_does_not_place_a_bomb_without_an_escape() {
    let rows = [
        "#######", "#.+...#", "#+....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let s = duel(&rows, (1, 1), (5, 5));
    let i = decide(&s);
    assert!(!i.bomb, "boxed in by blocks, a bomb would be suicide");
}

#[test]
fn does_not_bomb_when_the_escape_is_too_slow() {
    // a dead-end corridor of length 1 beyond the bomb: both ends inside the blast
    let rows = [
        "#######", "#.+...#", "#.#...#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let mut s = duel(&rows, (1, 1), (5, 5));
    s.players[0].range = 8;
    // fine escape exists only through the long open path; shorten the fuse so it is too slow
    s.config.fuse_ticks = 3;
    assert!(!decide(&s).bomb);
}

#[test]
fn places_a_bomb_next_to_a_block_when_it_can_escape() {
    let rows = [
        "#######", "#.+...#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let s = duel(&rows, (1, 1), (5, 5));
    let i = decide(&s);
    assert!(i.bomb);
    assert_eq!(
        i.mv, None,
        "moving first would drop the bomb on the wrong tile"
    );
}

#[test]
fn places_a_bomb_when_an_opponent_is_in_range() {
    let s = duel(&OPEN, (1, 1), (3, 1));
    assert!(decide(&s).bomb);
}

#[test]
fn walks_toward_a_block_it_can_bomb() {
    let rows = [
        "#######", "#.....#", "#.....#", "#....+#", "#.....#", "#.....#", "#######",
    ];
    let s = duel(&rows, (1, 1), (1, 5));
    let i = decide(&s);
    assert!(i.mv.is_some() && !i.bomb);
}

#[test]
fn picks_up_a_nearby_powerup() {
    let mut s = duel(&OPEN, (1, 1), (5, 5));
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(2, 1),
        kind: eksplode_re_core::PowerUpKind::Range,
    });
    assert_eq!(decide(&s).mv, Some(Dir::Right));
}

#[test]
fn does_not_exceed_its_bomb_limit_or_stack_bombs() {
    let mut s = duel(&OPEN, (1, 1), (3, 1));
    add_bomb(&mut s, 0, 1, 1, 40);
    assert!(
        !decide(&s).bomb,
        "already one bomb active and standing on it"
    );
}

#[test]
fn a_dead_bot_does_nothing() {
    let mut s = duel(&OPEN, (1, 1), (3, 1));
    s.players[0].alive = false;
    assert_eq!(decide(&s), PlayerInput::default());
}

#[test]
fn never_walks_into_a_blast_it_can_avoid() {
    let mut s = duel(&OPEN, (1, 3), (5, 5));
    add_bomb(&mut s, 1, 3, 3, 12);
    run_bot(&mut s, 30);
    assert!(s.players[0].alive);
}

fn input_after(s: &GameState) -> PlayerInput {
    DefaultBot::new(1).decide(&s.observe(pid(0)))
}

#[test]
fn bonuses_attract_the_bot_but_maluses_do_not() {
    use eksplode_re_core::{PowerUp, PowerUpKind};
    // a punch two squares up and a slow two squares down: the bot goes for the punch
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    s.powerups.push(PowerUp {
        pos: pos(3, 1),
        kind: PowerUpKind::Punch,
    });
    s.powerups.push(PowerUp {
        pos: pos(3, 5),
        kind: PowerUpKind::Slow,
    });
    s.players[1].alive = true;
    for _ in 0..6 {
        let i = input_after(&s);
        if i.mv.is_some() {
            assert_eq!(i.mv, Some(Dir::Up), "{i:?}");
            break;
        }
        step(&mut s, &[PlayerInput::default(); 4]);
    }
}

#[test]
fn a_bot_walks_around_maluses() {
    use eksplode_re_core::{PowerUp, PowerUpKind};
    for kind in [
        PowerUpKind::Slow,
        PowerUpKind::HyperSpeed,
        PowerUpKind::SmallBomb,
    ] {
        let mut s = duel(&OPEN, (1, 1), (5, 5));
        s.players[0].bombs_max = 1;
        for (x, y) in [(2, 1), (1, 2), (3, 3), (2, 2)] {
            s.powerups.push(PowerUp {
                pos: pos(x, y),
                kind,
            });
        }
        run_bot(&mut s, 120);
        assert!(s.players[0].malus.is_none(), "{kind:?}");
    }
}

#[test]
fn bots_survive_every_power_on_the_board_and_never_punch() {
    use eksplode_re_core::{PowerUp, PowerUpKind};
    let mut s = duel(&OPEN, (1, 1), (5, 5));
    s.players[0].has_punch = true;
    for (i, kind) in PowerUpKind::ALL.iter().enumerate() {
        s.powerups.push(PowerUp {
            pos: pos(2 + (i as u8 % 3), 2 + (i as u8 / 3)),
            kind: *kind,
        });
    }
    let mut bot = DefaultBot::new(1);
    for _ in 0..200 {
        let i = bot.decide(&s.observe(pid(0)));
        assert!(!i.punch, "bots do not punch in this version");
        let mut inputs = [PlayerInput::default(); 4];
        inputs[0] = i;
        step(&mut s, &inputs);
    }
}

/// 7x7 open arena whose pattern starts at tick 100 and announces a square every 4 ticks with a
/// lead of 20 ticks.
fn pattern_state(starts: [Option<(u8, u8)>; 4]) -> GameState {
    let config = eksplode_re_core::Config {
        round_limit_ticks: 200,
        closing_ticks: 100,
        closing_lead_ticks: 20,
        ..eksplode_re_core::Config::default()
    };
    state_with(config, &OPEN, starts)
}

#[test]
fn f4_us2_a_bot_on_an_announced_square_moves_off_it_in_time() {
    // (1,1) is the first square of the spiral: announced at tick 100, lands at tick 120
    let mut s = pattern_state([Some((1, 1)), Some((3, 3)), Some((4, 3)), None]);
    let mut bot = DefaultBot::new(1);
    while s.tick < 100 {
        step(&mut s, &[PlayerInput::default(); 4]);
    }
    let mut dodged = false;
    for _ in 0..40 {
        let mut inputs = [PlayerInput::default(); 4];
        inputs[0] = bot.decide(&s.observe(pid(0)));
        let ev = step(&mut s, &inputs);
        dodged |= ev.contains(&Event::BlockLanded { pos: pos(1, 1) });
        assert!(s.players[0].alive, "crushed at tick {}", s.tick);
    }
    assert!(dodged, "the first block landed while the bot lived");
    assert_ne!(s.players[0].pos, pos(1, 1));
}

#[test]
fn f4_us2_a_bot_keeps_moving_inward_ahead_of_the_pattern() {
    let mut s = pattern_state([Some((1, 1)), Some((3, 3)), Some((4, 3)), None]);
    let mut bot = DefaultBot::new(2);
    let mut inputs = [PlayerInput::default(); 4];
    while s.tick < 170 && s.players[0].alive && s.players[1].alive && s.players[2].alive {
        inputs[0] = bot.decide(&s.observe(pid(0)));
        step(&mut s, &inputs);
    }
    assert!(s.players[0].alive, "dead at tick {}", s.tick);
}

fn team_state_of(friendly_fire: bool, starts: [Option<(u8, u8)>; 4]) -> GameState {
    let config = eksplode_re_core::Config {
        mode: eksplode_re_core::Mode::Team,
        friendly_fire,
        ..eksplode_re_core::Config::default()
    };
    state_with(config, &OPEN, starts)
}

#[test]
fn f4_us3_as6_a_bot_does_not_target_its_teammate() {
    // bot 0 and its teammate 1 stand next to each other; the opponents are far away
    let starts = [Some((3, 3)), Some((3, 2)), Some((1, 5)), Some((5, 5))];
    let s = team_state_of(true, starts);
    assert!(!decide(&s).bomb, "a bomb would only hit the teammate");
    let s = team_state_of(false, starts);
    assert!(
        !decide(&s).bomb,
        "and a teammate is no target even when friendly fire is off"
    );

    // the same bot bombs an opponent that stands there instead
    let starts = [Some((3, 3)), Some((5, 1)), Some((3, 2)), Some((5, 5))];
    assert!(
        decide(&team_state_of(true, starts)).bomb,
        "an opponent next to it is a target"
    );
}

#[test]
fn f4_us3_as6_a_bot_does_not_bomb_when_a_teammate_would_be_hit_with_friendly_fire_on() {
    // an opponent and the teammate are both within reach of a bomb on (3,3)
    let starts = [Some((3, 3)), Some((3, 4)), Some((3, 2)), Some((5, 5))];
    assert!(
        !decide(&team_state_of(true, starts)).bomb,
        "the teammate would die too"
    );
    assert!(
        decide(&team_state_of(false, starts)).bomb,
        "with friendly fire off only the opponent is hit"
    );
}

#[test]
fn f4_us3_as6_a_bot_still_flees_the_bomb_of_its_teammate() {
    let starts = [Some((3, 3)), Some((1, 1)), Some((5, 1)), Some((5, 5))];
    let mut s = team_state_of(false, starts);
    add_bomb(&mut s, 1, 3, 2, 20);
    let mut bot = DefaultBot::new(1);
    let mut inputs = [PlayerInput::default(); 4];
    for _ in 0..40 {
        inputs[0] = bot.decide(&s.observe(pid(0)));
        step(&mut s, &inputs);
    }
    assert!(s.players[0].alive);
    assert_ne!(s.players[0].pos, pos(3, 3), "it left the line of the bomb");
}

fn chaser_state(starts: [Option<(u8, u8)>; 4]) -> GameState {
    let config = eksplode_re_core::Config {
        mode: eksplode_re_core::Mode::Chaser,
        ..eksplode_re_core::Config::default()
    };
    // the first participant is the chaser
    state_with(config, &OPEN, starts)
}

fn decide_as(s: &GameState, slot: u8) -> PlayerInput {
    DefaultBot::new(1).decide(&s.observe(pid(slot)))
}

fn manhattan(a: eksplode_re_core::Pos, b: eksplode_re_core::Pos) -> u32 {
    u32::from(a.x.abs_diff(b.x) + a.y.abs_diff(b.y))
}

#[test]
fn f4_us4_as8_a_chaser_bot_bombs_a_runner_in_reach_and_walks_toward_the_others() {
    // a runner next to the chaser: bomb it, there is room to get away
    let s = chaser_state([Some((3, 3)), Some((3, 4)), Some((1, 1)), Some((5, 1))]);
    assert!(decide_as(&s, 0).bomb, "a runner is within reach");

    // the runners are far: it walks toward the nearest one
    let s = chaser_state([Some((3, 3)), Some((5, 5)), None, None]);
    let input = decide_as(&s, 0);
    assert!(!input.bomb);
    let dir = input.mv.expect("it moves");
    let (dx, dy) = dir.delta();
    let to = eksplode_re_core::Pos::new((3 + i16::from(dx)) as u8, (3 + i16::from(dy)) as u8);
    assert!(
        manhattan(to, pos(5, 5)) < manhattan(pos(3, 3), pos(5, 5)),
        "{dir:?}"
    );
}

#[test]
fn f4_us4_as8_a_chaser_bot_does_not_bomb_without_a_way_out_and_keeps_two_bombs_in_mind() {
    // the chaser stands in a dead end with a runner next to it: a bomb would be suicide
    let rows = [
        "#######", "#.#####", "#.#####", "#.....#", "#.#####", "#.#####", "#######",
    ];
    let s = state_with(
        eksplode_re_core::Config {
            mode: eksplode_re_core::Mode::Chaser,
            ..eksplode_re_core::Config::default()
        },
        &rows,
        [Some((1, 1)), Some((1, 2)), Some((5, 3)), None],
    );
    assert!(
        !decide_as(&s, 0).bomb || { s.players[0].hearts > 1 },
        "{:?}",
        decide_as(&s, 0)
    );
}

#[test]
fn f4_us4_as8_a_runner_bot_flees_the_chaser_and_never_bombs() {
    for (me, chaser) in [
        ((3, 3), (3, 5)),
        ((3, 3), (1, 3)),
        ((2, 2), (4, 2)),
        ((5, 3), (3, 3)),
    ] {
        let s = chaser_state([Some(chaser), Some(me), None, None]);
        let input = decide_as(&s, 1);
        assert!(!input.bomb && !input.punch);
        let dir = input
            .mv
            .unwrap_or_else(|| panic!("a runner at {me:?} must move away from {chaser:?}"));
        let (dx, dy) = dir.delta();
        let to = eksplode_re_core::Pos::new(
            (i16::from(me.0) + i16::from(dx)) as u8,
            (i16::from(me.1) + i16::from(dy)) as u8,
        );
        assert!(
            manhattan(to, pos(chaser.0, chaser.1))
                > manhattan(pos(me.0, me.1), pos(chaser.0, chaser.1)),
            "{me:?} from {chaser:?} moved {dir:?}"
        );
    }
}

#[test]
fn f4_us4_as8_a_runner_bot_collects_a_bonus_in_reach_when_the_chaser_is_far() {
    let mut s = chaser_state([Some((5, 5)), Some((1, 1)), None, None]);
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(1, 3),
        kind: eksplode_re_core::PowerUpKind::Speed,
    });
    let input = decide_as(&s, 1);
    assert_eq!(input.mv, Some(Dir::Down), "toward the bonus");
}

#[test]
fn f4_us4_as8_a_runner_with_the_punch_sends_a_bomb_in_front_of_it_away() {
    let mut s = chaser_state([Some((5, 5)), Some((3, 3)), None, None]);
    s.players[1].has_punch = true;
    s.players[1].facing = Dir::Right;
    add_bomb(&mut s, 0, 4, 3, 40);
    let input = decide_as(&s, 1);
    assert!(input.punch, "{input:?}");
    // without the punch it does not
    s.players[1].has_punch = false;
    assert!(!decide_as(&s, 1).punch);
}

#[test]
fn f4_us4_as8_a_runner_bot_still_flees_the_chasers_bomb() {
    let mut s = chaser_state([Some((1, 1)), Some((3, 3)), None, None]);
    add_bomb(&mut s, 0, 3, 2, 20);
    let mut bot = DefaultBot::new(1);
    let mut inputs = [PlayerInput::default(); 4];
    for _ in 0..40 {
        inputs[1] = bot.decide(&s.observe(pid(1)));
        step(&mut s, &inputs);
    }
    assert!(s.players[1].alive);
}

// Feature 012: behaviours and difficulty.

use eksplode_re_bots::{Behaviour, Profile};
use eksplode_re_core::BotDifficulty;

const WIDE: [&str; 7] = [
    "###############",
    "#.............#",
    "#.............#",
    "#.............#",
    "#.............#",
    "#.............#",
    "###############",
];

fn bot_of(behaviour: Behaviour, difficulty: BotDifficulty) -> DefaultBot {
    DefaultBot::with_profile(
        1,
        Profile {
            behaviour,
            difficulty,
        },
    )
}

fn decide_with(behaviour: Behaviour, s: &GameState, slot: u8) -> PlayerInput {
    bot_of(behaviour, BotDifficulty::Hard).decide(&s.observe(pid(slot)))
}

fn target(from: eksplode_re_core::Pos, dir: Dir) -> eksplode_re_core::Pos {
    let (dx, dy) = dir.delta();
    eksplode_re_core::Pos::new(
        (i16::from(from.x) + i16::from(dx)) as u8,
        (i16::from(from.y) + i16::from(dy)) as u8,
    )
}

#[test]
fn f12_us1_as1_pick_gives_each_bot_one_of_three_behaviours() {
    let mut seen = [0u32; 3];
    for seed in 0..300u64 {
        let b = Behaviour::pick(seed);
        let i = Behaviour::ALL.iter().position(|x| *x == b).unwrap();
        seen[i] += 1;
    }
    assert!(seen.iter().all(|n| *n > 0), "{seen:?}");
}

#[test]
fn f12_us1_as2_pick_is_reproducible() {
    for seed in [0, 1, 7, 12345, u64::MAX] {
        assert_eq!(Behaviour::pick(seed), Behaviour::pick(seed));
        for d in BotDifficulty::ALL {
            let p = Profile::pick(seed, d);
            assert_eq!(p.difficulty, d);
            assert_eq!(p.behaviour, Behaviour::pick(seed));
        }
    }
}

#[test]
fn f12_behaviour_names_round_trip() {
    let names: Vec<_> = Behaviour::ALL.iter().map(|b| b.name()).collect();
    assert_eq!(names, ["aggressive", "protective", "balanced"]);
    for b in Behaviour::ALL {
        assert_eq!(Behaviour::from_name(b.name()), Some(b));
    }
    assert_eq!(Behaviour::from_name("random"), None);
    assert_eq!(Behaviour::from_name("Aggressive"), None);
    assert_eq!(Behaviour::Protective.label(), "Protective");
}

#[test]
fn f12_us1_as3_aggressive_heads_for_a_reachable_opponent() {
    let mut s = duel(&WIDE, (7, 3), (13, 3));
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(5, 3),
        kind: eksplode_re_core::PowerUpKind::Range,
    });
    assert_eq!(
        decide_with(Behaviour::Aggressive, &s, 0).mv,
        Some(Dir::Right)
    );
    assert_eq!(decide_with(Behaviour::Balanced, &s, 0).mv, Some(Dir::Left));
    assert_eq!(DefaultBot::new(1).decide(&obs(&s, 0)).mv, Some(Dir::Left));
}

#[test]
fn f12_us1_aggressive_approaches_a_far_opponent_when_idle() {
    // nothing to collect or dig and the opponent beyond any bombing square within 14 steps
    let s = duel(&WIDE, (1, 1), (13, 5));
    let i = decide_with(Behaviour::Aggressive, &s, 0);
    let dir = i.mv.expect("it moves toward the opponent");
    assert!(
        manhattan(target(pos(1, 1), dir), pos(13, 5)) < manhattan(pos(1, 1), pos(13, 5)),
        "{dir:?}"
    );
}

#[test]
fn f12_us1_as4_protective_prefers_powerups_and_keeps_off_opponents() {
    let mut s = duel(&WIDE, (7, 3), (11, 3));
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(4, 3),
        kind: eksplode_re_core::PowerUpKind::Range,
    });
    assert_eq!(
        decide_with(Behaviour::Protective, &s, 0).mv,
        Some(Dir::Left)
    );
    assert_eq!(
        decide_with(Behaviour::Aggressive, &s, 0).mv,
        Some(Dir::Right)
    );

    // the only power-up lies next to the opponent: a Protective bot does not step beside it
    let mut s = duel(&WIDE, (9, 2), (10, 3));
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(9, 3),
        kind: eksplode_re_core::PowerUpKind::Range,
    });
    assert_eq!(decide_with(Behaviour::Balanced, &s, 0).mv, Some(Dir::Down));
    let i = decide_with(Behaviour::Protective, &s, 0);
    if let Some(dir) = i.mv {
        assert!(
            manhattan(target(pos(9, 2), dir), pos(10, 3)) > 1,
            "{dir:?} ends next to the opponent"
        );
    }
}

#[test]
fn f12_us1_protective_backs_away_from_a_close_opponent() {
    // no power-up, no block, no bomb: an opponent two squares away makes it step back
    let mut s = duel(&WIDE, (7, 3), (9, 3));
    s.players[0].bombs_max = 0;
    let i = decide_with(Behaviour::Protective, &s, 0);
    let dir = i.mv.expect("it moves away");
    assert!(manhattan(target(pos(7, 3), dir), pos(9, 3)) > 2, "{dir:?}");
}

/// Runs a bot with `profile` as `slot` while everybody else stands still.
fn run_profile(s: &mut GameState, profile: Profile, slot: u8, ticks: u32) {
    let mut bot = DefaultBot::with_profile(1, profile);
    for _ in 0..ticks {
        let mut inputs = [PlayerInput::default(); 4];
        inputs[usize::from(slot)] = bot.decide(&s.observe(pid(slot)));
        step(s, &inputs);
    }
}

#[test]
fn f12_us1_behaviours_keep_the_safety_layer() {
    for behaviour in Behaviour::ALL {
        let hard = Profile {
            behaviour,
            difficulty: BotDifficulty::Hard,
        };
        // flees a blast
        let mut s = duel(&OPEN, (3, 3), (5, 5));
        add_bomb(&mut s, 1, 3, 2, 20);
        run_profile(&mut s, hard, 0, 40);
        assert!(s.players[0].alive, "{behaviour:?} flees");

        // flees a chain reaction
        let mut s = duel(&OPEN, (5, 5), (1, 1));
        add_bomb(&mut s, 1, 3, 3, 3);
        add_bomb(&mut s, 1, 5, 3, 60);
        s.players[1].bombs_max = 2;
        run_profile(&mut s, hard, 0, 25);
        assert!(s.players[0].alive, "{behaviour:?} chain");

        // no bomb without a way out
        let rows = [
            "#######", "#.+...#", "#+....#", "#.....#", "#.....#", "#.....#", "#######",
        ];
        let s = duel(&rows, (1, 1), (5, 5));
        assert!(
            !decide_with(behaviour, &s, 0).bomb,
            "{behaviour:?} boxed in"
        );

        // friendly fire on: no bomb that reaches a teammate
        let starts = [Some((3, 3)), Some((3, 4)), Some((3, 2)), Some((5, 5))];
        assert!(
            !decide_with(behaviour, &team_state_of(true, starts), 0).bomb,
            "{behaviour:?} teammate"
        );

        // moves off an announced square in time
        let mut s = pattern_state([Some((1, 1)), Some((3, 3)), Some((4, 3)), None]);
        while s.tick < 100 {
            step(&mut s, &[PlayerInput::default(); 4]);
        }
        run_profile(&mut s, hard, 0, 40);
        assert!(s.players[0].alive, "{behaviour:?} pattern");
        assert_ne!(s.players[0].pos, pos(1, 1));

        // a runner moves away from the chaser and never bombs
        let s = chaser_state([Some((3, 5)), Some((3, 3)), None, None]);
        let i = decide_with(behaviour, &s, 1);
        assert!(!i.bomb, "{behaviour:?} runner bomb");
        let dir = i.mv.expect("a runner moves away");
        assert!(
            manhattan(target(pos(3, 3), dir), pos(3, 5)) > 2,
            "{behaviour:?} runner {dir:?}"
        );
    }
}

#[test]
fn f12_us1_a_protective_runner_flees_the_chaser_from_farther_away() {
    // the chaser is 6 squares away: beyond a Balanced runner's flee distance, not a Protective one's
    let s = state_with(
        eksplode_re_core::Config {
            mode: eksplode_re_core::Mode::Chaser,
            ..eksplode_re_core::Config::default()
        },
        &WIDE,
        [Some((13, 3)), Some((7, 3)), None, None],
    );
    let protective = decide_with(Behaviour::Protective, &s, 1);
    let dir = protective.mv.expect("a protective runner moves away");
    assert!(manhattan(target(pos(7, 3), dir), pos(13, 3)) > 6, "{dir:?}");
}

use eksplode_re_bots::Skill;

fn skill(think_every: u32, react_ticks: u32, mistake_chance: f32) -> Skill {
    Skill {
        think_every,
        react_ticks,
        mistake_chance,
    }
}

#[test]
fn f12_skill_table_is_ordered() {
    assert_eq!(Skill::of(BotDifficulty::Easy), skill(6, 10, 0.20));
    assert_eq!(Skill::of(BotDifficulty::Medium), skill(3, 5, 0.06));
    assert_eq!(Skill::of(BotDifficulty::Hard), skill(1, 0, 0.0));
    for pair in BotDifficulty::ALL.windows(2) {
        let (a, b) = (Skill::of(pair[0]), Skill::of(pair[1]));
        assert!(a.think_every >= b.think_every, "{pair:?}");
        assert!(a.react_ticks >= b.react_ticks, "{pair:?}");
        assert!(a.mistake_chance >= b.mistake_chance, "{pair:?}");
    }
}

/// Bot 0 at (3,3) on the line of a bomb of player 1 at (3,2) that is `age` ticks old.
fn threatened(age: u32) -> GameState {
    let mut s = duel(&OPEN, (3, 3), (5, 5));
    let fuse = s.config.fuse_ticks;
    add_bomb(&mut s, 1, 3, 2, fuse - age);
    s
}

fn flees(i: PlayerInput) -> bool {
    i.mv.is_some_and(|d| target(pos(3, 3), d).y == 3 && target(pos(3, 3), d).x != 3)
}

#[test]
fn f12_us2_as1_hard_reacts_at_once() {
    for seed in 0..20 {
        let mut bot = DefaultBot::with_profile(
            seed,
            Profile {
                behaviour: Behaviour::Balanced,
                difficulty: BotDifficulty::Hard,
            },
        );
        assert!(flees(bot.decide(&obs(&threatened(0), 0))), "seed {seed}");
    }
}

#[test]
fn f12_us2_as2_easy_and_medium_react_late() {
    for difficulty in [BotDifficulty::Easy, BotDifficulty::Medium] {
        let react = Skill::of(difficulty).react_ticks;
        let profile = Profile {
            behaviour: Behaviour::Balanced,
            difficulty,
        };
        let mut fled = 0;
        for seed in 0..20 {
            // too young: the bot decides as if the bomb were not there
            let young =
                DefaultBot::with_profile(seed, profile).decide(&obs(&threatened(react - 1), 0));
            let without = DefaultBot::with_profile(seed, profile)
                .decide(&obs(&duel(&OPEN, (3, 3), (5, 5)), 0));
            assert_eq!(young, without, "{difficulty:?} seed {seed}");
            // old enough: it runs, unless this decision is one of its mistakes
            let old = DefaultBot::with_profile(seed, profile).decide(&obs(&threatened(react), 0));
            fled += u32::from(flees(old));
        }
        assert!(fled >= 14, "{difficulty:?} fled {fled}/20");
    }
}

#[test]
fn f12_us2_own_bombs_and_flames_are_seen_at_once() {
    let profile = Profile {
        behaviour: Behaviour::Balanced,
        difficulty: BotDifficulty::Easy,
    };
    let mut moved = 0;
    for seed in 0..20 {
        let mut s = duel(&OPEN, (3, 3), (5, 5));
        let fuse = s.config.fuse_ticks;
        add_bomb(&mut s, 0, 3, 3, fuse);
        moved += u32::from(
            DefaultBot::with_profile(seed, profile)
                .decide(&obs(&s, 0))
                .mv
                .is_some(),
        );
    }
    assert!(moved >= 14, "it left its own fresh bomb {moved}/20 times");
}

#[test]
fn f12_us2_think_interval_holds_the_last_move() {
    let profile = Profile {
        behaviour: Behaviour::Balanced,
        difficulty: BotDifficulty::Easy,
    };
    let every = Skill::of(BotDifficulty::Easy).think_every;
    let mut s = duel(&WIDE, (1, 3), (13, 5));
    s.powerups.push(eksplode_re_core::PowerUp {
        pos: pos(6, 3),
        kind: eksplode_re_core::PowerUpKind::Range,
    });
    let mut checked = 0;
    for seed in 0..20 {
        let mut bot = DefaultBot::with_profile(seed, profile);
        let first = bot.decide(&obs(&s, 0));
        let Some(dir) = first.mv else { continue };
        checked += 1;
        for _ in 1..every {
            let held = bot.decide(&obs(&s, 0));
            assert_eq!(held.mv, Some(dir), "seed {seed}");
            assert!(!held.bomb && !held.punch);
        }
        // a burning square ahead drops the held move
        let mut bot = DefaultBot::with_profile(seed, profile);
        bot.decide(&obs(&s, 0));
        let mut burning = s.clone();
        burning.flames.push(eksplode_re_core::Flame {
            pos: target(pos(1, 3), dir),
            ticks_left: 5,
            owners: 0,
        });
        assert_eq!(bot.decide(&obs(&burning, 0)), PlayerInput::default());
    }
    assert!(checked >= 10, "only {checked} seeds started with a move");
}

#[test]
fn f12_us2_as4_hard_makes_no_mistakes() {
    assert_eq!(Skill::of(BotDifficulty::Hard).mistake_chance, 0.0);
    // Hard bots of every behaviour play the same moves as the same bot fed the same states
    for behaviour in Behaviour::ALL {
        let profile = Profile {
            behaviour,
            difficulty: BotDifficulty::Hard,
        };
        let mut s = state(&WIDE, [Some((1, 1)), Some((13, 5)), None, None]);
        let (mut a, mut b) = (
            DefaultBot::with_profile(3, profile),
            DefaultBot::with_profile(3, profile),
        );
        for _ in 0..200 {
            let o = obs(&s, 0);
            let i = a.decide(&o);
            assert_eq!(i, b.decide(&o));
            let mut inputs = [PlayerInput::default(); 4];
            inputs[0] = i;
            step(&mut s, &inputs);
        }
    }
}

#[test]
fn f12_us2_as5_mistakes_never_step_into_fire() {
    let profile = Profile {
        behaviour: Behaviour::Balanced,
        difficulty: BotDifficulty::Easy,
    };
    for seed in 0..200 {
        let mut s = duel(&OPEN, (3, 3), (5, 5));
        for (x, y) in [(3, 2), (2, 3)] {
            s.flames.push(eksplode_re_core::Flame {
                pos: pos(x, y),
                ticks_left: 8,
                owners: 0,
            });
        }
        let mut bot = DefaultBot::with_profile(seed, profile);
        for _ in 0..12 {
            if let Some(dir) = bot.decide(&obs(&s, 0)).mv {
                let to = target(pos(3, 3), dir);
                assert!(
                    !s.flames.iter().any(|f| f.pos == to),
                    "seed {seed} stepped into fire at {to:?}"
                );
            }
        }
    }
}

#[test]
fn f12_us2_difficulty_is_reproducible() {
    for difficulty in [BotDifficulty::Easy, BotDifficulty::Medium] {
        let run = || {
            let mut s = eksplode_re_core::GameState::new(
                eksplode_re_core::Config::default(),
                11,
                [true; 4],
            )
            .unwrap();
            let mut bots: Vec<DefaultBot> = (0..4)
                .map(|i| DefaultBot::with_profile(40 + i, Profile::pick(40 + i, difficulty)))
                .collect();
            let mut log = Vec::new();
            for _ in 0..600 {
                let mut inputs = [PlayerInput::default(); 4];
                for (i, bot) in bots.iter_mut().enumerate() {
                    inputs[i] = bot.decide(&s.observe(pid(i as u8)));
                }
                log.push(inputs);
                step(&mut s, &inputs);
            }
            log
        };
        assert_eq!(run(), run(), "{difficulty:?}");
    }
}
