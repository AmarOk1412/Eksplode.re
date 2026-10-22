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
