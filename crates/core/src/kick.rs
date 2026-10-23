//! Kicking bombs: a player with the kick who walks into a landed bomb makes it slide, one square
//! every `slide_ticks`, until the next square is blocked.

use crate::action::{Dir, Pos};
use crate::arena::Arena;
use crate::punch::free_square;
use crate::state::{Bomb, Event, GameState, Participant, Slide};

/// Where bomb `idx` will come to rest if nothing changes: the last free square in the direction
/// of its slide. A bomb that does not slide stays where it is. Bots use this to see where a kicked
/// bomb will explode.
pub fn predict_slide_stop(
    arena: &Arena,
    players: &[Participant],
    bombs: &[Bomb],
    idx: usize,
) -> Pos {
    let Some(bomb) = bombs.get(idx) else {
        return Pos::new(0, 0);
    };
    let Some(slide) = bomb.slide else {
        return bomb.pos;
    };
    let (w, h) = (arena.width(), arena.height());
    let mut at = bomb.pos;
    while let Some(next) = at.step(slide.dir, w, h) {
        if !free_square(arena, players, bombs, idx, next) {
            break;
        }
        at = next;
    }
    at
}

/// Where bomb `idx` will be when its fuse ends, if nothing changes: it slid as many squares as
/// fit in `ticks_left`, but no farther than its stop square.
pub fn predict_slide_explosion(
    arena: &Arena,
    players: &[Participant],
    bombs: &[Bomb],
    idx: usize,
    slide_ticks: u32,
) -> Pos {
    let (Some(bomb), true) = (bombs.get(idx), slide_ticks > 0) else {
        return predict_slide_stop(arena, players, bombs, idx);
    };
    let Some(slide) = bomb.slide else {
        return bomb.pos;
    };
    let steps = if bomb.ticks_left >= slide.step_left {
        1 + (bomb.ticks_left - slide.step_left) / slide_ticks
    } else {
        0
    };
    let (w, h) = (arena.width(), arena.height());
    let mut at = bomb.pos;
    for _ in 0..steps {
        match at.step(slide.dir, w, h) {
            Some(next) if free_square(arena, players, bombs, idx, next) => at = next,
            _ => break,
        }
    }
    at
}

/// Called when player `i` tried to walk onto `to` and was blocked: a living player with the kick
/// pushes a landed bomb there if the square behind it is free. Does nothing otherwise.
pub(crate) fn start(state: &mut GameState, i: usize, to: Pos, dir: Dir, events: &mut Vec<Event>) {
    if !state.players[i].alive || !state.players[i].has_kick {
        return;
    }
    let Some(idx) = state
        .bombs
        .iter()
        .position(|b| b.pos == to && b.flight.is_none() && b.slide.is_none())
    else {
        return;
    };
    let Some(behind) = to.step(dir, state.arena.width(), state.arena.height()) else {
        return;
    };
    if !free_square(&state.arena, &state.players, &state.bombs, idx, behind) {
        return;
    }
    // the tick of the kick counts as one of the ticks until the first step
    state.bombs[idx].slide = Some(Slide {
        dir,
        step_left: state.config.slide_ticks + 1,
    });
    state.players[i].cooldown_left = state.players[i].effective_cooldown();
    events.push(Event::BombKicked {
        player: state.players[i].id,
        from: to,
        dir,
    });
}

/// Moves every sliding bomb: counts down to its next step, steps, and stops it when the square
/// ahead is blocked.
pub(crate) fn advance(state: &mut GameState, events: &mut Vec<Event>) {
    let (w, h) = (state.arena.width(), state.arena.height());
    for idx in 0..state.bombs.len() {
        let Some(mut slide) = state.bombs[idx].slide else {
            continue;
        };
        slide.step_left = slide.step_left.saturating_sub(1);
        if slide.step_left > 0 {
            state.bombs[idx].slide = Some(slide);
            continue;
        }
        let here = state.bombs[idx].pos;
        let next = here
            .step(slide.dir, w, h)
            .filter(|n| free_square(&state.arena, &state.players, &state.bombs, idx, *n));
        match next {
            Some(n) => {
                slide.step_left = state.config.slide_ticks;
                state.bombs[idx].pos = n;
                state.bombs[idx].slide = Some(slide);
                events.push(Event::BombSlid { pos: n });
            }
            None => {
                state.bombs[idx].slide = None;
                events.push(Event::BombStopped { pos: here });
            }
        }
    }
}
