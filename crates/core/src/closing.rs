//! The end pattern: during the last `closing_ticks` of a Normal or Team round, blocks fall on the
//! board in a clockwise spiral from the outer ring inwards until one player (or team) is left.
//!
//! Each square is announced `closing_lead_ticks` before its block lands. Everything is counted
//! in ticks and the order comes from the board size alone, so the pattern is deterministic and
//! draws no random numbers.

use crate::action::Pos;
use crate::arena::Tile;
use crate::room::Mode;
use crate::state::{Event, Falling, GameState};

/// Whether the end pattern runs in this round: Normal and Team mode with at least two
/// participants, and a pattern that is shorter than the round limit.
pub fn enabled(state: &GameState) -> bool {
    let c = &state.config;
    matches!(c.mode, Mode::Normal | Mode::Team)
        && state.started > 1
        && c.closing_ticks > 0
        && c.closing_ticks < c.round_limit_ticks
}

/// The tick at which the pattern starts.
pub fn start_tick(state: &GameState) -> u32 {
    state.config.round_limit_ticks - state.config.closing_ticks
}

/// Ticks between two announcements so that `squares` squares fit in `closing_ticks`.
pub fn interval(closing_ticks: u32, squares: u32) -> u32 {
    if squares == 0 {
        return 1;
    }
    (closing_ticks / squares).max(1)
}

/// Every inner square of a `width` x `height` board in the order they are covered: the top row
/// from the left, the right column down, the bottom row from the right, the left column up, then
/// the next ring inwards, and so on to the middle.
pub fn closing_order(width: u8, height: u8) -> Vec<Pos> {
    let mut out = Vec::new();
    let (mut x0, mut y0) = (1i16, 1i16);
    let (mut x1, mut y1) = (i16::from(width) - 2, i16::from(height) - 2);
    while x0 <= x1 && y0 <= y1 {
        let at = |x: i16, y: i16| Pos::new(x as u8, y as u8);
        for x in x0..=x1 {
            out.push(at(x, y0));
        }
        for y in y0 + 1..=y1 {
            out.push(at(x1, y));
        }
        if y1 > y0 {
            for x in (x0..x1).rev() {
                out.push(at(x, y1));
            }
        }
        if x1 > x0 {
            for y in (y0 + 1..y1).rev() {
                out.push(at(x0, y));
            }
        }
        x0 += 1;
        y0 += 1;
        x1 -= 1;
        y1 -= 1;
    }
    out
}

/// One tick of the pattern: starts it at its tick, counts the announced squares down and lands
/// the blocks that are due, then announces the next square. Called after detonation so a bomb
/// whose fuse ends in the tick a block lands still explodes.
pub fn advance(state: &mut GameState, events: &mut Vec<Event>) {
    if !enabled(state) || state.tick < start_tick(state) {
        return;
    }
    if !state.closing.started {
        let n = closing_order(state.arena.width(), state.arena.height())
            .into_iter()
            .filter(|p| state.arena.tile(*p) != Tile::Wall)
            .count() as u32;
        state.closing.order = closing_order(state.arena.width(), state.arena.height());
        state.closing.started = true;
        state.closing.elapsed = 0;
        state.closing.next = 0;
        state.closing.every = interval(state.config.closing_ticks, n);
        events.push(Event::PatternStarted);
    }

    for f in &mut state.closing.falling {
        f.ticks_left = f.ticks_left.saturating_sub(1);
    }
    let (due, waiting): (Vec<Falling>, Vec<Falling>) = std::mem::take(&mut state.closing.falling)
        .into_iter()
        .partition(|f| f.ticks_left == 0);
    state.closing.falling = waiting;
    for f in due {
        land(state, f.pos, events);
    }

    if state.closing.elapsed % state.closing.every == 0 {
        while let Some(pos) = state.closing.order.get(state.closing.next).copied() {
            state.closing.next += 1;
            if state.arena.tile(pos) == Tile::Wall {
                continue;
            }
            state.closing.falling.push(Falling {
                pos,
                ticks_left: state.config.closing_lead_ticks,
            });
            events.push(Event::BlockWarned { pos });
            break;
        }
    }
    state.closing.elapsed += 1;
}

/// The block lands: the tile becomes a wall, a player on it is eliminated and a bomb, a power-up
/// or a flame on it is removed. A bomb in the air is not touched.
fn land(state: &mut GameState, pos: Pos, events: &mut Vec<Event>) {
    state.arena.set_tile(pos, Tile::Wall);
    events.push(Event::BlockLanded { pos });

    for p in &mut state.players {
        if p.alive && p.pos == pos {
            p.alive = false;
            p.malus = None;
            events.push(Event::PlayerEliminated { player: p.id });
        }
    }
    let mut released = Vec::new();
    state.bombs.retain(|b| {
        let gone = b.pos == pos && b.flight.is_none();
        if gone {
            released.push(b.owner.index());
        }
        !gone
    });
    for owner in released {
        let p = &mut state.players[owner];
        p.bombs_active = p.bombs_active.saturating_sub(1);
    }
    state.powerups.retain(|u| {
        let gone = u.pos == pos;
        if gone {
            events.push(Event::PowerUpDestroyed { pos });
        }
        !gone
    });
    state.flames.retain(|f| f.pos != pos);
}
