#![allow(dead_code, clippy::unwrap_used)]

use bomberust_core::{
    Arena, Bomb, Config, Flame, GameState, Observation, PlayerId, Pos, PowerUp, PowerUpKind,
};

pub const OPEN: [&str; 7] = [
    "#######", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
];

pub fn pid(i: u8) -> PlayerId {
    PlayerId::new(i).unwrap()
}

/// A fresh four-player game on the standard arena.
pub fn standard() -> GameState {
    GameState::new(Config::default(), 7, [true; 4]).unwrap()
}

/// A small custom arena with the given start positions for players 0 and 1.
pub fn small(rows: &[&str], p0: (u8, u8), p1: (u8, u8)) -> GameState {
    let arena = Arena::from_rows(rows).unwrap();
    let starts = [
        Some(Pos::new(p0.0, p0.1)),
        Some(Pos::new(p1.0, p1.1)),
        None,
        None,
    ];
    GameState::custom(Config::default(), arena, starts, 1).unwrap()
}

pub fn obs(s: &GameState) -> Observation {
    s.observe(pid(0))
}

pub fn bomb(s: &mut GameState, x: u8, y: u8, ticks_left: u32) {
    s.bombs.push(Bomb {
        owner: pid(0),
        pos: Pos::new(x, y),
        ticks_left,
        range: 2,
    });
}

pub fn flame(s: &mut GameState, x: u8, y: u8) {
    s.flames.push(Flame {
        pos: Pos::new(x, y),
        ticks_left: 5,
    });
}

pub fn powerup(s: &mut GameState, x: u8, y: u8, kind: PowerUpKind) {
    s.powerups.push(PowerUp {
        pos: Pos::new(x, y),
        kind,
    });
}
