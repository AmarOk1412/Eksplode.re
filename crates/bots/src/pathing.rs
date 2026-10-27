//! Breadth-first search over tiles that respects bomb timing and the move cooldown.

use std::collections::BTreeSet;

use eksplode_re_core::{Dir, Observation, Pos, Tile};

use crate::danger::DangerMap;

/// What to search from and with which constraints.
pub struct PathRequest<'a> {
    pub obs: &'a Observation,
    pub danger: &'a DangerMap,
    /// Step of the first possible move (steps count from the next tick).
    pub earliest_move: u32,
    /// Direction preference, used to break ties.
    pub order: [Dir; 4],
    pub max_depth: u32,
    /// Do not step on a malus power-up (it is never worth it unless fleeing).
    pub avoid_malus: bool,
}

impl<'a> PathRequest<'a> {
    pub fn new(obs: &'a Observation, danger: &'a DangerMap) -> Self {
        let me = &obs.players[obs.me.index()];
        Self {
            obs,
            danger,
            earliest_move: me.cooldown_left.max(1),
            order: Dir::ALL,
            max_depth: 14,
            avoid_malus: true,
        }
    }
}

struct Node {
    pos: Pos,
    depth: u32,
    parent: usize,
    dir: Option<Dir>,
}

/// Shortest path from the player to a tile accepted by `goal`.
///
/// Every tile on the way must be free of fire while the player stands on it, and the goal
/// must stay free of fire for good. `Some(vec![])` means the current tile already qualifies.
pub fn find_path(req: &PathRequest, goal: &dyn Fn(Pos) -> bool) -> Option<Vec<Dir>> {
    let obs = req.obs;
    let me = &obs.players[obs.me.index()];
    let arrival = |k: u32| {
        if k == 0 {
            0
        } else {
            req.earliest_move + (k - 1) * me.effective_cooldown()
        }
    };

    let mut nodes = vec![Node {
        pos: me.pos,
        depth: 0,
        parent: 0,
        dir: None,
    }];
    let mut seen = BTreeSet::from([me.pos]);
    let mut head = 0;
    while head < nodes.len() {
        let idx = head;
        head += 1;
        let (pos, depth) = (nodes[idx].pos, nodes[idx].depth);
        let from = arrival(depth).max(1);

        if goal(pos) && req.danger.safe_from(pos, from) {
            let mut path = Vec::new();
            let mut i = idx;
            while let Some(d) = nodes[i].dir {
                path.push(d);
                i = nodes[i].parent;
            }
            path.reverse();
            return Some(path);
        }
        if depth >= req.max_depth {
            continue;
        }
        let leaves_at = arrival(depth + 1);
        if req
            .danger
            .dangerous_during(pos, from, leaves_at.saturating_sub(1))
        {
            continue;
        }
        for dir in req.order {
            let Some(next) = pos.step(dir, obs.arena.width(), obs.arena.height()) else {
                continue;
            };
            if passable(obs, next)
                && !(req.avoid_malus && has_malus(obs, next))
                && seen.insert(next)
            {
                nodes.push(Node {
                    pos: next,
                    depth: depth + 1,
                    parent: idx,
                    dir: Some(dir),
                });
            }
        }
    }
    None
}

/// Does a malus power-up lie on `p`?
pub fn has_malus(obs: &Observation, p: Pos) -> bool {
    obs.powerups.iter().any(|u| u.pos == p && u.kind.is_malus())
}

/// Can the player step on `p` right now (ignoring fire)?
pub fn passable(obs: &Observation, p: Pos) -> bool {
    obs.arena.tile(p) == Tile::Floor
        && !obs.bombs.iter().any(|b| b.flight.is_none() && b.pos == p)
        && !obs
            .players
            .iter()
            .any(|o| o.alive && o.id != obs.me && o.pos == p)
}
