//! Bomb placement and detonation, including chain reactions.

use std::collections::BTreeSet;

use crate::action::{Dir, Pos};
use crate::arena::{Arena, Tile};
use crate::state::{Bomb, Event, Flame, GameState, PowerUp};

/// Places a bomb for player `i` if the rules allow it.
pub(crate) fn place(state: &mut GameState, i: usize, events: &mut Vec<Event>) {
    let p = &state.players[i];
    let occupied = state
        .bombs
        .iter()
        .any(|b| b.pos == p.pos && b.flight.is_none());
    if !p.alive || p.bombs_active >= p.bombs_max || occupied {
        return;
    }
    let bomb = Bomb {
        owner: p.id,
        pos: p.pos,
        ticks_left: state.config.fuse_ticks,
        range: p.placed_range(),
        flight: None,
    };
    events.push(Event::BombPlaced {
        player: p.id,
        pos: p.pos,
    });
    state.bombs.push(bomb);
    state.players[i].bombs_active += 1;
}

/// Explodes every bomb whose fuse ran out, plus every bomb its flames reach.
pub(crate) fn detonate(state: &mut GameState, events: &mut Vec<Event>) {
    let mut exploding: Vec<bool> = state.bombs.iter().map(|b| b.ticks_left == 0).collect();
    if !exploding.contains(&true) {
        return;
    }

    let mut tiles = BTreeSet::new();
    let mut destroyed = BTreeSet::new();
    let mut done = vec![false; exploding.len()];
    loop {
        let mut progressed = false;
        for idx in 0..exploding.len() {
            if !exploding[idx] || done[idx] {
                continue;
            }
            done[idx] = true;
            progressed = true;
            let b = &state.bombs[idx];
            let hit = blast(&state.arena, b.pos, b.range);
            for (j, other) in state.bombs.iter().enumerate() {
                // a bomb in the air is out of reach of flames
                if other.flight.is_none() && hit.tiles.contains(&other.pos) {
                    exploding[j] = true;
                }
            }
            if b.flight.is_some() && state.arena.tile(b.pos) == Tile::Block {
                destroyed.insert(b.pos);
            }
            tiles.extend(hit.tiles);
            destroyed.extend(hit.destroyed);
        }
        if !progressed {
            break;
        }
    }

    for (bomb, _) in state.bombs.iter().zip(&exploding).filter(|(_, e)| **e) {
        events.push(Event::BombExploded {
            owner: bomb.owner,
            pos: bomb.pos,
        });
        let owner = &mut state.players[bomb.owner.index()];
        owner.bombs_active = owner.bombs_active.saturating_sub(1);
    }
    let mut keep = exploding.iter().map(|e| !e);
    state.bombs.retain(|_| keep.next().unwrap_or(true));

    for pos in &destroyed {
        state.arena.set_tile(*pos, Tile::Floor);
        events.push(Event::BlockDestroyed { pos: *pos });
    }
    let mut revealed: Vec<PowerUp> = Vec::new();
    for pos in &destroyed {
        if let Some(kind) = state.drops.roll(&mut state.rng) {
            revealed.push(PowerUp { pos: *pos, kind });
        }
    }

    state.powerups.retain(|u| {
        let burns = tiles.contains(&u.pos);
        if burns {
            events.push(Event::PowerUpDestroyed { pos: u.pos });
        }
        !burns
    });
    for u in revealed {
        events.push(Event::PowerUpSpawned {
            pos: u.pos,
            kind: u.kind,
        });
        state.powerups.push(u);
    }

    for pos in tiles {
        match state.flames.iter_mut().find(|f| f.pos == pos) {
            Some(f) => f.ticks_left = state.config.flame_ticks,
            None => state.flames.push(Flame {
                pos,
                ticks_left: state.config.flame_ticks,
            }),
        }
    }
}

/// The tiles a bomb burns and the blocks it destroys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blast {
    pub tiles: BTreeSet<Pos>,
    pub destroyed: BTreeSet<Pos>,
}

/// Blast of a bomb at `pos`: its own tile plus four rays that stop at walls and at the
/// first block. Rays pass over other bombs, which then detonate too.
pub fn blast(arena: &Arena, pos: Pos, range: u8) -> Blast {
    let mut out = Blast::default();
    out.tiles.insert(pos);
    for dir in Dir::ALL {
        let mut cur = pos;
        for _ in 0..range {
            let Some(next) = cur.step(dir, arena.width(), arena.height()) else {
                break;
            };
            match arena.tile(next) {
                Tile::Wall => break,
                Tile::Block => {
                    out.tiles.insert(next);
                    out.destroyed.insert(next);
                    break;
                }
                Tile::Floor => {
                    out.tiles.insert(next);
                    cur = next;
                }
            }
        }
    }
    out
}
