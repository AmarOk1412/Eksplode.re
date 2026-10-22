//! Bomb placement and detonation, including chain reactions.

use std::collections::BTreeSet;

use crate::action::{Dir, Pos};
use crate::arena::{Arena, Tile};
use crate::state::{Bomb, Event, Flame, GameState, PowerUp};

/// Places a bomb for player `i` if the rules allow it.
pub(crate) fn place(state: &mut GameState, i: usize, events: &mut Vec<Event>) {
    let p = &state.players[i];
    if !p.alive || p.bombs_active >= p.bombs_max || state.bombs.iter().any(|b| b.pos == p.pos) {
        return;
    }
    let bomb = Bomb {
        owner: p.id,
        pos: p.pos,
        ticks_left: state.config.fuse_ticks,
        range: p.range,
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
            let mut reached = BTreeSet::new();
            cast(
                &state.arena,
                &state.bombs[idx],
                &mut reached,
                &mut destroyed,
            );
            for (j, other) in state.bombs.iter().enumerate() {
                if reached.contains(&other.pos) {
                    exploding[j] = true;
                }
            }
            tiles.extend(reached);
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
    let revealed: Vec<PowerUp> = destroyed
        .iter()
        .filter_map(|pos| {
            state
                .hidden_powerups
                .remove(pos)
                .map(|kind| PowerUp { pos: *pos, kind })
        })
        .collect();

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

/// Blast of one bomb: its own tile plus four rays that stop at walls and at the first block.
fn cast(arena: &Arena, bomb: &Bomb, tiles: &mut BTreeSet<Pos>, destroyed: &mut BTreeSet<Pos>) {
    tiles.insert(bomb.pos);
    for dir in Dir::ALL {
        let mut cur = bomb.pos;
        for _ in 0..bomb.range {
            let Some(next) = cur.step(dir, arena.width(), arena.height()) else {
                break;
            };
            match arena.tile(next) {
                Tile::Wall => break,
                Tile::Block => {
                    tiles.insert(next);
                    destroyed.insert(next);
                    break;
                }
                Tile::Floor => {
                    tiles.insert(next);
                    cur = next;
                }
            }
        }
    }
}
