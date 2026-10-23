//! One simulation tick.

use crate::action::{PlayerInput, MAX_PLAYERS};
use crate::arena::Tile;
use crate::bomb;
use crate::closing;
use crate::kick;
use crate::punch;
use crate::state::{ActiveMalus, Event, GameState, Outcome, Phase, PowerUpKind};

/// Advances the round by one tick and returns what happened.
///
/// Order: cooldowns, flame and fuse timers, malus timers, moves (rotating priority), bomb
/// flights (hops and landings), slides (kicked bombs), punches, bomb placement, detonation with chain reactions (drops
/// are decided for destroyed blocks), eliminations, end-of-round check. Does nothing once the
/// round has ended.
pub fn step(state: &mut GameState, inputs: &[PlayerInput; MAX_PLAYERS]) -> Vec<Event> {
    let mut events = Vec::new();
    if state.phase != Phase::Playing {
        return events;
    }

    for p in state.players.iter_mut().filter(|p| p.alive) {
        p.cooldown_left = p.cooldown_left.saturating_sub(1);
    }
    for f in &mut state.flames {
        f.ticks_left = f.ticks_left.saturating_sub(1);
    }
    state.flames.retain(|f| f.ticks_left > 0);
    for b in &mut state.bombs {
        b.ticks_left = b.ticks_left.saturating_sub(1);
    }

    malus_timers(state, &mut events);

    let first = state.tick as usize % MAX_PLAYERS;
    let order = [0, 1, 2, 3].map(|i| (first + i) % MAX_PLAYERS);
    for &i in &order {
        move_player(state, i, inputs[i], &mut events);
    }
    punch::advance(state, &mut events);
    kick::advance(state, &mut events);
    for &i in &order {
        if inputs[i].punch {
            punch::apply(state, i, &mut events);
        }
    }
    for &i in &order {
        if inputs[i].bomb {
            bomb::place(state, i, &mut events);
        }
    }

    bomb::detonate(state, &mut events);
    closing::advance(state, &mut events);
    eliminate(state, &mut events);

    state.tick += 1;
    if let Some(outcome) = outcome(state) {
        state.phase = Phase::Ended(outcome);
        events.push(Event::RoundEnded(outcome));
    }
    events
}

fn malus_timers(state: &mut GameState, events: &mut Vec<Event>) {
    for p in state.players.iter_mut().filter(|p| p.alive) {
        let Some(mut malus) = p.malus else {
            continue;
        };
        malus.ticks_left = malus.ticks_left.saturating_sub(1);
        if malus.ticks_left == 0 {
            p.malus = None;
            events.push(Event::MalusEnded {
                player: p.id,
                kind: malus.kind,
            });
        } else {
            p.malus = Some(malus);
        }
    }
}

fn move_player(state: &mut GameState, i: usize, input: PlayerInput, events: &mut Vec<Event>) {
    let (id, from, alive, cooldown_left) = {
        let p = &state.players[i];
        (p.id, p.pos, p.alive, p.cooldown_left)
    };
    let (Some(dir), true) = (input.mv, alive) else {
        return;
    };
    state.players[i].facing = dir;
    if cooldown_left > 0 {
        return;
    }
    let Some(to) = from.step(dir, state.arena.width(), state.arena.height()) else {
        return;
    };
    let blocked = state.arena.tile(to) != Tile::Floor
        || state
            .bombs
            .iter()
            .any(|b| b.pos == to && b.flight.is_none())
        || state.players.iter().any(|o| o.alive && o.pos == to);
    if blocked {
        kick::start(state, i, to, dir, events);
        return;
    }
    state.players[i].pos = to;
    state.players[i].cooldown_left = state.players[i].effective_cooldown();
    events.push(Event::Moved {
        player: id,
        from,
        to,
    });

    if let Some(idx) = state.powerups.iter().position(|u| u.pos == to) {
        let power = state.powerups.remove(idx);
        let cfg = &state.config;
        let p = &mut state.players[i];
        match power.kind {
            PowerUpKind::ExtraBomb => p.bombs_max = (p.bombs_max + 1).min(cfg.max_bombs),
            PowerUpKind::Range => p.range = (p.range + 1).min(cfg.max_range),
            PowerUpKind::Speed => {
                p.move_cooldown = p.move_cooldown.saturating_sub(1).max(cfg.min_move_cooldown);
            }
            PowerUpKind::Punch => p.has_punch = true,
            PowerUpKind::Kick => p.has_kick = true,
            PowerUpKind::Slow | PowerUpKind::HyperSpeed | PowerUpKind::SmallBomb => {
                p.malus = Some(ActiveMalus {
                    kind: power.kind,
                    ticks_left: cfg.malus_ticks,
                });
                events.push(Event::MalusStarted {
                    player: id,
                    kind: power.kind,
                });
            }
        }
        events.push(Event::PowerUpCollected {
            player: id,
            pos: to,
            kind: power.kind,
        });
    }
}

fn eliminate(state: &mut GameState, events: &mut Vec<Event>) {
    for i in 0..MAX_PLAYERS {
        let p = &state.players[i];
        if p.alive && state.flames.iter().any(|f| f.pos == p.pos) {
            events.push(Event::PlayerEliminated { player: p.id });
            state.players[i].alive = false;
            state.players[i].malus = None;
        }
    }
}

/// A single starting participant plays a practice round that only ends by death or time.
fn outcome(state: &GameState) -> Option<Outcome> {
    let alive: Vec<_> = state.players.iter().filter(|p| p.alive).collect();
    match alive.as_slice() {
        [] => Some(Outcome::Draw),
        [only] if state.started > 1 => Some(Outcome::Winner(only.id)),
        // with the end pattern the round ends when at most one is left, however long it takes
        _ if closing::enabled(state) => None,
        _ if state.tick >= state.config.round_limit_ticks => Some(Outcome::Draw),
        _ => None,
    }
}
