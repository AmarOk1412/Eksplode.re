//! The default bot: flee fire, grab power-ups, hunt opponents, break blocks.

use bomberust_core::{blast, Dir, Observation, Participant, Phase, PlayerInput, Pos, Rng};

use crate::danger::DangerMap;
use crate::pathing::{find_path, has_malus, passable, PathRequest};
use crate::Bot;

/// Chance per tick to take a random safe step when nothing else is worth doing.
const WANDER_CHANCE: f32 = 0.3;

/// A deterministic bot; the seed only breaks ties between equally good moves.
pub struct DefaultBot {
    rng: Rng,
}

impl DefaultBot {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
        }
    }

    fn shuffled_dirs(&mut self) -> [Dir; 4] {
        let mut dirs = Dir::ALL;
        for i in (1..dirs.len()).rev() {
            let j = self.rng.next_below(i as u32 + 1) as usize;
            dirs.swap(i, j);
        }
        dirs
    }

    /// No way out: step toward the tile that burns last, or stay.
    fn desperate(
        &mut self,
        obs: &Observation,
        me: &Participant,
        danger: &DangerMap,
    ) -> PlayerInput {
        let best = self
            .shuffled_dirs()
            .into_iter()
            .filter_map(|d| {
                let t = me.pos.step(d, obs.arena.width(), obs.arena.height())?;
                passable(obs, t).then(|| (danger.first_danger(t).unwrap_or(u32::MAX), d))
            })
            .max_by_key(|(when, _)| *when);
        let here = danger.first_danger(me.pos).unwrap_or(u32::MAX);
        match best {
            Some((when, dir)) if when > here => PlayerInput {
                mv: Some(dir),
                bomb: false,
                ..PlayerInput::default()
            },
            _ => PlayerInput::default(),
        }
    }

    fn wander(&mut self, obs: &Observation, me: &Participant, danger: &DangerMap) -> PlayerInput {
        if !self.rng.chance(WANDER_CHANCE) {
            return PlayerInput::default();
        }
        for d in self.shuffled_dirs() {
            let Some(t) = me.pos.step(d, obs.arena.width(), obs.arena.height()) else {
                continue;
            };
            if passable(obs, t) && !has_malus(obs, t) && danger.safe_from(t, 1) {
                return PlayerInput {
                    mv: Some(d),
                    bomb: false,
                    ..PlayerInput::default()
                };
            }
        }
        PlayerInput::default()
    }
}

impl Bot for DefaultBot {
    fn decide(&mut self, obs: &Observation) -> PlayerInput {
        let me = &obs.players[obs.me.index()];
        if !me.alive || obs.phase != Phase::Playing {
            return PlayerInput::default();
        }
        let danger = DangerMap::from_observation(obs);
        let order = self.shuffled_dirs();
        let mut req = PathRequest::new(obs, &danger);
        req.order = order;
        let step_to = |path: Option<Vec<Dir>>| {
            path.and_then(|p| p.first().copied()).map(|d| PlayerInput {
                mv: Some(d),
                bomb: false,
                ..PlayerInput::default()
            })
        };

        if !danger.safe_from(me.pos, 1) {
            // staying alive beats avoiding a malus
            req.avoid_malus = false;
            return step_to(find_path(&req, &|_| true))
                .unwrap_or_else(|| self.desperate(obs, me, &danger));
        }
        if can_bomb_here(obs, me) {
            return PlayerInput {
                mv: None,
                bomb: true,
                ..PlayerInput::default()
            };
        }

        req.max_depth = 8;
        let powerup = |t: Pos| {
            obs.powerups
                .iter()
                .any(|u| u.pos == t && !u.kind.is_malus())
        };
        if let Some(i) = step_to(find_path(&req, &powerup)) {
            return i;
        }

        req.max_depth = 14;
        if me.bombs_active < me.bombs_max {
            let enemies: Vec<Pos> = obs
                .players
                .iter()
                .filter(|p| p.alive && p.id != me.id)
                .map(|p| p.pos)
                .collect();
            let hunt = |t: Pos| {
                t != me.pos
                    && roomy(obs, t)
                    && blast(&obs.arena, t, me.placed_range())
                        .tiles
                        .iter()
                        .any(|x| enemies.contains(x))
            };
            if let Some(i) = step_to(find_path(&req, &hunt)) {
                return i;
            }
            let dig = |t: Pos| {
                t != me.pos
                    && roomy(obs, t)
                    && !blast(&obs.arena, t, me.placed_range()).destroyed.is_empty()
            };
            if let Some(i) = step_to(find_path(&req, &dig)) {
                return i;
            }
        }
        self.wander(obs, me, &danger)
    }
}

/// A tile with at least two open neighbours is not a dead end.
fn roomy(obs: &Observation, t: Pos) -> bool {
    Dir::ALL
        .iter()
        .filter_map(|d| t.step(*d, obs.arena.width(), obs.arena.height()))
        .filter(|n| passable(obs, *n))
        .count()
        >= 2
}

/// Is a bomb here useful and can the bot still get away from it?
fn can_bomb_here(obs: &Observation, me: &Participant) -> bool {
    if me.bombs_active >= me.bombs_max || obs.bombs.iter().any(|b| b.pos == me.pos) {
        return false;
    }
    let hit = blast(&obs.arena, me.pos, me.placed_range());
    let useful = !hit.destroyed.is_empty()
        || obs
            .players
            .iter()
            .any(|p| p.alive && p.id != me.id && hit.tiles.contains(&p.pos));
    if !useful {
        return false;
    }
    // The bomb is placed during the next tick and explodes `fuse_ticks` ticks later.
    let danger = DangerMap::with_bomb(obs, me.pos, me.placed_range(), obs.config.fuse_ticks + 1);
    let mut req = PathRequest::new(obs, &danger);
    req.earliest_move = me.cooldown_left.max(2);
    find_path(&req, &|_| true).is_some()
}
