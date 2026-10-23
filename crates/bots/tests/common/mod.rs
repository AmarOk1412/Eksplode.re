#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use bomberust_core::{Arena, Bomb, Config, GameState, Observation, PlayerId, Pos};

pub const OPEN: [&str; 7] = [
    "#######", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
];

pub fn pid(i: u8) -> PlayerId {
    PlayerId::new(i).expect("valid slot")
}

pub fn pos(x: u8, y: u8) -> Pos {
    Pos::new(x, y)
}

pub fn state(rows: &[&str], starts: [Option<(u8, u8)>; 4]) -> GameState {
    let arena = Arena::from_rows(rows).expect("valid arena");
    let starts = starts.map(|s| s.map(|(x, y)| Pos::new(x, y)));
    GameState::custom(Config::default(), arena, starts, 1).expect("valid state")
}

/// Player 0 at `me`, player 1 at `other`.
pub fn duel(rows: &[&str], me: (u8, u8), other: (u8, u8)) -> GameState {
    state(rows, [Some(me), Some(other), None, None])
}

pub fn add_bomb(s: &mut GameState, owner: u8, x: u8, y: u8, ticks_left: u32) {
    let range = s.players[usize::from(owner)].range;
    s.bombs.push(Bomb {
        owner: pid(owner),
        pos: pos(x, y),
        ticks_left,
        range,
        flight: None,
        slide: None,
    });
    s.players[usize::from(owner)].bombs_active += 1;
}

pub fn obs(s: &GameState, me: u8) -> Observation {
    s.observe(pid(me))
}
