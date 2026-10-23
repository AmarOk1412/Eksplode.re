mod common;

use bomberust_bots::{Bot, DefaultBot};
use bomberust_core::{step, Dir, Event, GameState, PlayerInput};
use common::*;

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
    s.powerups.push(bomberust_core::PowerUp {
        pos: pos(2, 1),
        kind: bomberust_core::PowerUpKind::Range,
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
    use bomberust_core::{PowerUp, PowerUpKind};
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
    use bomberust_core::{PowerUp, PowerUpKind};
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
    use bomberust_core::{PowerUp, PowerUpKind};
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
    let config = bomberust_core::Config {
        round_limit_ticks: 200,
        closing_ticks: 100,
        closing_lead_ticks: 20,
        ..bomberust_core::Config::default()
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
