//! Punching bombs: a punched bomb hops one square at a time over anything in the way and lands on
//! the first square without a block, wall, living player or other bomb.

use crate::action::Pos;
use crate::arena::{Arena, Tile};
use crate::state::{Bomb, Event, Flight, GameState, Participant};

/// Whether bomb `idx` could rest on `at`: a floor tile with no living player and no other bomb,
/// landed or flying.
pub(crate) fn free_square(
    arena: &Arena,
    players: &[Participant],
    bombs: &[Bomb],
    idx: usize,
    at: Pos,
) -> bool {
    arena.tile(at) == Tile::Floor
        && !players.iter().any(|p| p.alive && p.pos == at)
        && !bombs
            .iter()
            .enumerate()
            .any(|(j, b)| j != idx && b.pos == at)
}

fn is_free(state: &GameState, idx: usize, at: Pos) -> bool {
    free_square(&state.arena, &state.players, &state.bombs, idx, at)
}

/// Where the bomb `idx` would land if nothing changed: the first free square after its own in the
/// direction of its flight, or, at the edge of the arena, the nearest free square back toward its
/// origin. A bomb that is not in flight stays where it is. Bots use this to see where a punched
/// bomb will explode.
pub fn predict_landing(arena: &Arena, players: &[Participant], bombs: &[Bomb], idx: usize) -> Pos {
    let Some(bomb) = bombs.get(idx) else {
        return Pos::new(0, 0);
    };
    let Some(flight) = bomb.flight else {
        return bomb.pos;
    };
    let (w, h) = (arena.width(), arena.height());
    let mut at = bomb.pos;
    while let Some(next) = at.step(flight.dir, w, h) {
        at = next;
        if free_square(arena, players, bombs, idx, at) {
            return at;
        }
    }
    let back = flight.dir.opposite();
    loop {
        if free_square(arena, players, bombs, idx, at) {
            return at;
        }
        if at == flight.origin {
            return bomb.pos;
        }
        match at.step(back, w, h) {
            Some(next) => at = next,
            None => return bomb.pos,
        }
    }
}

/// A living player with the punch power who presses punch sends the landed bomb on the faced
/// square on its way. Does nothing in any other case.
pub(crate) fn apply(state: &mut GameState, i: usize, events: &mut Vec<Event>) {
    let p = &state.players[i];
    if !p.alive || !p.has_punch {
        return;
    }
    let (id, dir) = (p.id, p.facing);
    let Some(front) = p.pos.step(dir, state.arena.width(), state.arena.height()) else {
        return;
    };
    let hop = state.config.punch_hop_ticks;
    let Some(bomb) = state
        .bombs
        .iter_mut()
        .find(|b| b.pos == front && b.flight.is_none())
    else {
        return;
    };
    bomb.slide = None;
    bomb.flight = Some(Flight {
        dir,
        hop_left: hop,
        origin: front,
    });
    events.push(Event::BombPunched {
        player: id,
        from: front,
        dir,
    });
}

/// Moves every bomb in flight: counts down to its next hop, hops, and lands it when the square
/// it reaches is free. At the edge of the arena it lands on the nearest free square back toward
/// its origin.
pub(crate) fn advance(state: &mut GameState, events: &mut Vec<Event>) {
    let (w, h) = (state.arena.width(), state.arena.height());
    for idx in 0..state.bombs.len() {
        let Some(mut flight) = state.bombs[idx].flight else {
            continue;
        };
        flight.hop_left = flight.hop_left.saturating_sub(1);
        if flight.hop_left > 0 {
            state.bombs[idx].flight = Some(flight);
            continue;
        }
        flight.hop_left = state.config.punch_hop_ticks;
        let here = state.bombs[idx].pos;
        match here.step(flight.dir, w, h) {
            Some(next) => {
                state.bombs[idx].pos = next;
                events.push(Event::BombHopped { pos: next });
                if is_free(state, idx, next) {
                    land(state, idx, next, events);
                } else {
                    state.bombs[idx].flight = Some(flight);
                }
            }
            None => match back_to_free(state, idx, here, &flight) {
                Some(at) => land(state, idx, at, events),
                None => state.bombs[idx].flight = Some(flight),
            },
        }
    }
}

fn land(state: &mut GameState, idx: usize, at: Pos, events: &mut Vec<Event>) {
    let bomb = &mut state.bombs[idx];
    bomb.pos = at;
    bomb.flight = None;
    events.push(Event::BombLanded { pos: at });
}

/// The nearest free square from `from` back to the origin of the flight (the origin included).
fn back_to_free(state: &GameState, idx: usize, from: Pos, flight: &Flight) -> Option<Pos> {
    let (w, h) = (state.arena.width(), state.arena.height());
    let back = flight.dir.opposite();
    let mut at = from;
    loop {
        if is_free(state, idx, at) {
            return Some(at);
        }
        if at == flight.origin {
            return None;
        }
        at = at.step(back, w, h)?;
    }
}
