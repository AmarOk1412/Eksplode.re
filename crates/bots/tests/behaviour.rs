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
