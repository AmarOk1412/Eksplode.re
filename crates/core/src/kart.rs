//! Deterministic outside-board geometry and eliminated-player kart actions.

use crate::{
    closing, punch, Arena, Bomb, Dir, Event, Flight, GameState, KartState, Mode, Participant,
    Phase, Pos,
};

/// Predicted landing for a new throw under the current public occupancy.
pub fn predict_throw_landing(
    arena: &Arena,
    players: &[Participant],
    bombs: &[Bomb],
    index: u16,
) -> Option<Pos> {
    let (origin, direction) = launch_lane(arena.width(), arena.height(), index)?;
    let mut at = origin;
    while let Some(next) = at.step(direction, arena.width(), arena.height()) {
        at = next;
        if punch::free_square(arena, players, bombs, usize::MAX, at) {
            return Some(at);
        }
    }
    loop {
        if punch::free_square(arena, players, bombs, usize::MAX, at) {
            return Some(at);
        }
        if at == origin {
            return None;
        }
        at = at.step(direction.opposite(), arena.width(), arena.height())?;
    }
}

/// Number of clockwise positions on the virtual perimeter, or zero for invalid dimensions.
pub fn perimeter_len(width: u8, height: u8) -> u16 {
    if width < 3 || height < 3 {
        return 0;
    }
    2 * (u16::from(width) + u16::from(height)) - 4
}

/// Virtual border coordinate and inward aim; coordinates do not occupy arena tiles.
pub fn track_position(width: u8, height: u8, index: u16) -> Option<(Pos, Dir)> {
    if index >= perimeter_len(width, height) {
        return None;
    }
    let horizontal = u16::from(width) - 1;
    let vertical = u16::from(height) - 1;
    if index < horizontal {
        Some((Pos::new(index as u8, 0), Dir::Down))
    } else if index < horizontal + vertical {
        Some((Pos::new(width - 1, (index - horizontal) as u8), Dir::Left))
    } else if index < 2 * horizontal + vertical {
        Some((
            Pos::new((2 * horizontal + vertical - index) as u8, height - 1),
            Dir::Up,
        ))
    } else {
        Some((
            Pos::new(0, (2 * horizontal + 2 * vertical - index) as u8),
            Dir::Right,
        ))
    }
}

/// Closest perimeter position, using clockwise edge order for equal distances.
pub fn nearest_index(width: u8, height: u8, from: Pos) -> Option<u16> {
    (0..perimeter_len(width, height)).min_by_key(|index| {
        let Some((pos, _)) = track_position(width, height, *index) else {
            return u16::MAX;
        };
        u16::from(pos.x.abs_diff(from.x)) + u16::from(pos.y.abs_diff(from.y))
    })
}

/// First interior square and throw direction, clamping corner lanes inward.
pub fn launch_lane(width: u8, height: u8, index: u16) -> Option<(Pos, Dir)> {
    let (pos, dir) = track_position(width, height, index)?;
    let first = match dir {
        Dir::Down => Pos::new(pos.x.clamp(1, width - 2), 1),
        Dir::Left => Pos::new(width - 2, pos.y.clamp(1, height - 2)),
        Dir::Up => Pos::new(pos.x.clamp(1, width - 2), height - 2),
        Dir::Right => Pos::new(1, pos.y.clamp(1, height - 2)),
    };
    Some((first, dir))
}

/// Kart sprite center on an exterior rectangle, in tile-space coordinates.
pub fn view_position(width: u8, height: u8, index: u16) -> Option<(f32, f32)> {
    let (pos, dir) = track_position(width, height, index)?;
    let x = f32::from(pos.x) / f32::from(width - 1) * (f32::from(width) + 1.0) - 0.5;
    let y = f32::from(pos.y) / f32::from(height - 1) * (f32::from(height) + 1.0) - 0.5;
    Some(match dir {
        Dir::Down => (x, -0.5),
        Dir::Left => (f32::from(width) + 0.5, y),
        Dir::Up => (x, f32::from(height) + 0.5),
        Dir::Right => (-0.5, y),
    })
}

pub(crate) fn resolve_returns(state: &mut GameState) {
    if !enabled(state) {
        return;
    }
    for slot in 0..crate::MAX_PLAYERS {
        let Some(preferred) = state.players[slot]
            .kart
            .and_then(|kart| kart.pending_return)
        else {
            continue;
        };
        let width = state.arena.width();
        let height = state.arena.height();
        let landing = (1..height.saturating_sub(1))
            .flat_map(|row| (1..width.saturating_sub(1)).map(move |column| Pos::new(column, row)))
            .filter(|pos| {
                state.arena.tile(*pos) == crate::Tile::Floor
                    && !state.bombs.iter().any(|bomb| bomb.pos == *pos)
                    && !state
                        .players
                        .iter()
                        .any(|player| player.alive && player.pos == *pos)
                    && !state
                        .flames
                        .iter()
                        .any(|flame| flame.pos == *pos && flame.ticks_left > 0)
            })
            .min_by_key(|pos| {
                (
                    u16::from(pos.x.abs_diff(preferred.x)) + u16::from(pos.y.abs_diff(preferred.y)),
                    pos.y,
                    pos.x,
                )
            });
        let Some(landing) = landing else {
            continue;
        };
        let owner = state.players[slot].id;
        let owned = state
            .bombs
            .iter()
            .filter(|bomb| bomb.owner == owner)
            .fold(0u8, |count, _| count.saturating_add(1));
        let player = &mut state.players[slot];
        player.alive = true;
        player.pos = landing;
        player.kart = None;
        player.immobilized_left = 0;
        player.eliminated_order = None;
        player.bombs_max = state.config.start_bombs;
        player.bombs_active = owned;
        player.range = state.config.start_range;
        player.move_cooldown = state.config.base_move_cooldown;
        player.cooldown_left = 0;
        player.malus = None;
        player.has_punch = false;
        player.has_kick = false;
        player.hearts = 1;
        player.shield_left = 0;
        player.facing = Dir::Down;
    }
}

pub(crate) fn enabled(state: &GameState) -> bool {
    state.phase == Phase::Playing
        && state.started > 1
        && matches!(state.config.mode, Mode::Normal | Mode::Team)
        && !(closing::enabled(state)
            && (state.closing.started || state.tick >= closing::start_tick(state)))
}

pub(crate) fn move_kart(state: &mut GameState, slot: usize, movement: Option<Dir>) {
    if !enabled(state) {
        return;
    }
    let length = perimeter_len(state.arena.width(), state.arena.height());
    let Some(kart) = state.players[slot].kart.as_mut() else {
        return;
    };
    if length == 0 || kart.index >= length {
        return;
    }
    kart.cooldown_left = kart.cooldown_left.saturating_sub(1);
    kart.travel = movement.filter(|dir| matches!(dir, Dir::Left | Dir::Right));
    if kart.cooldown_left == 0 {
        match kart.travel {
            Some(Dir::Right) => kart.index = (kart.index + 1) % length,
            Some(Dir::Left) => kart.index = (kart.index + length - 1) % length,
            _ => return,
        }
        kart.cooldown_left = state.config.base_move_cooldown;
    }
}

pub(crate) fn enter(state: &mut GameState, slot: usize) {
    if !enabled(state) {
        return;
    }
    let Some(index) = nearest_index(
        state.arena.width(),
        state.arena.height(),
        state.players[slot].pos,
    ) else {
        return;
    };
    let player = &mut state.players[slot];
    player.kart = Some(KartState {
        index,
        entered_tick: state.tick,
        cooldown_left: 0,
        travel: None,
        pending_return: None,
    });
    player.immobilized_left = 0;
    player.malus = None;
    player.has_punch = false;
    player.has_kick = false;
    player.range = state.config.start_range;
    player.move_cooldown = state.config.base_move_cooldown;
}

pub(crate) fn launch(state: &mut GameState, slot: usize, events: &mut Vec<Event>) {
    if !enabled(state) {
        return;
    }
    let player = &state.players[slot];
    let Some(kart) = player.kart else {
        return;
    };
    if player.alive
        || state
            .bombs
            .iter()
            .any(|bomb| bomb.owner == player.id && bomb.revenge_stint == Some(kart.entered_tick))
    {
        return;
    }
    let Some((first, dir)) = launch_lane(state.arena.width(), state.arena.height(), kart.index)
    else {
        return;
    };
    if predict_throw_landing(&state.arena, &state.players, &state.bombs, kart.index).is_none() {
        return;
    }
    state.bombs.push(Bomb {
        owner: player.id,
        pos: first,
        ticks_left: state.config.fuse_ticks,
        range: state.config.start_range,
        flight: Some(Flight {
            dir,
            hop_left: state.config.punch_hop_ticks,
            origin: first,
        }),
        slide: None,
        revenge_stint: Some(kart.entered_tick),
    });
    events.push(Event::BombPlaced {
        player: player.id,
        pos: first,
    });
    let player = &mut state.players[slot];
    player.bombs_active = player.bombs_active.saturating_add(1);
    player.bombs_placed = player.bombs_placed.saturating_add(1);
}
