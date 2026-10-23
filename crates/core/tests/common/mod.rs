#![allow(dead_code, clippy::expect_used)]

use bomberust_core::{Arena, Config, GameState, PlayerId, PlayerInput, Pos};

/// 7x7 arena with an open interior.
pub const OPEN: [&str; 7] = [
    "#######", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
];

pub fn pid(i: u8) -> PlayerId {
    PlayerId::new(i).expect("valid slot")
}

pub fn pos(x: u8, y: u8) -> Pos {
    Pos::new(x, y)
}

pub fn idle() -> [PlayerInput; 4] {
    [PlayerInput::default(); 4]
}

pub fn bomb_input() -> PlayerInput {
    PlayerInput {
        mv: None,
        bomb: true,
        ..PlayerInput::default()
    }
}

/// Builds a state on a custom arena with the given start positions.
pub fn state(rows: &[&str], starts: [Option<(u8, u8)>; 4]) -> GameState {
    state_with(Config::default(), rows, starts)
}

pub fn state_with(config: Config, rows: &[&str], starts: [Option<(u8, u8)>; 4]) -> GameState {
    let arena = Arena::from_rows(rows).expect("valid arena");
    let starts = starts.map(|s| s.map(|(x, y)| Pos::new(x, y)));
    GameState::custom(config, arena, starts, 1).expect("valid state")
}

/// Two players parked away from the centre cross of a 7x7 arena.
pub fn two_safe() -> [Option<(u8, u8)>; 4] {
    [Some((1, 5)), Some((5, 1)), None, None]
}

/// Adds a bomb as if `owner` had just placed it.
pub fn add_bomb(s: &mut GameState, owner: u8, x: u8, y: u8, ticks_left: u32) {
    let range = s.players[usize::from(owner)].range;
    s.bombs.push(bomberust_core::Bomb {
        owner: pid(owner),
        pos: pos(x, y),
        ticks_left,
        range,
        flight: None,
        slide: None,
    });
    s.players[usize::from(owner)].bombs_active += 1;
}

pub fn only(slot: usize, x: u8, y: u8) -> [Option<(u8, u8)>; 4] {
    let mut s = [None; 4];
    s[slot] = Some((x, y));
    s
}
