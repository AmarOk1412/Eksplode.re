//! The default bot: flee fire, grab power-ups, hunt opponents, break blocks.

use eksplode_re_core::closing::closing_order;
use eksplode_re_core::{
    blast, Dir, Mode, Observation, Participant, Phase, PlayerInput, Pos, Rng, Role,
};

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

        if me.role == Role::Runner && me.has_punch {
            // a bomb in front of a runner with the punch is sent away while its fuse is long
            let front = me
                .pos
                .step(me.facing, obs.arena.width(), obs.arena.height());
            let threat = front.is_some_and(|f| {
                obs.bombs
                    .iter()
                    .any(|b| b.pos == f && b.flight.is_none() && b.ticks_left > 6)
            });
            if threat {
                return PlayerInput {
                    punch: true,
                    ..PlayerInput::default()
                };
            }
        }
        if !danger.safe_from(me.pos, 1) {
            // staying alive beats avoiding a malus
            req.avoid_malus = false;
            return step_to(find_path(&req, &|_| true))
                .unwrap_or_else(|| self.desperate(obs, me, &danger));
        }
        if me.role == Role::Runner {
            req.max_depth = 10;
            if let Some(i) = step_to(flee_chaser(obs, me, &req)) {
                return i;
            }
        }
        if let Some(progress) = closing_progress(obs) {
            req.max_depth = 14;
            if let Some(i) = step_to(move_inward(obs, me, &req, progress)) {
                return i;
            }
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
                .filter(|p| is_enemy(me, p))
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

/// How far the end pattern has got, from 0.0 at its start to 1.0 at the end of its time; `None`
/// before it starts and in rounds that have none.
fn closing_progress(obs: &Observation) -> Option<f32> {
    let c = &obs.config;
    if !matches!(c.mode, Mode::Normal | Mode::Team)
        || c.closing_ticks == 0
        || c.closing_ticks >= c.round_limit_ticks
    {
        return None;
    }
    let start = c.round_limit_ticks - c.closing_ticks;
    (obs.tick >= start).then(|| ((obs.tick - start) as f32 / c.closing_ticks as f32).min(1.0))
}

/// The squares are covered from the outside in, so a bot whose square is due soon walks toward
/// squares that come later: the path to the nearest one, or `None` when it is safe where it is.
fn move_inward(
    obs: &Observation,
    me: &Participant,
    req: &PathRequest,
    progress: f32,
) -> Option<Vec<Dir>> {
    let order = closing_order(obs.arena.width(), obs.arena.height());
    let width = usize::from(obs.arena.width());
    let mut rank = vec![None; width * usize::from(obs.arena.height())];
    for (i, p) in order.iter().enumerate() {
        rank[usize::from(p.y) * width + usize::from(p.x)] = Some(i);
    }
    let rank_of = |p: Pos| rank[usize::from(p.y) * width + usize::from(p.x)];
    let mine = rank_of(me.pos)?;
    // the announcement reaches this fifth of the order within a few seconds
    let horizon = ((progress + 0.2) * order.len() as f32).ceil() as usize;
    if mine >= horizon {
        return None;
    }
    let target = horizon.max(mine + 1);
    find_path(req, &|t| rank_of(t).is_some_and(|r| r >= target))
        .or_else(|| find_path(req, &|t| rank_of(t).is_some_and(|r| r > mine)))
        .filter(|p| !p.is_empty())
}

/// Another living participant the bot may hurt: the runners for the chaser, nobody for a runner
/// (it has no bombs), the other team in Team mode and everybody else without teams.
fn is_enemy(me: &Participant, other: &Participant) -> bool {
    if !other.alive || other.id == me.id {
        return false;
    }
    match me.role {
        Role::Chaser => other.role == Role::Runner,
        Role::Runner => false,
        Role::Normal => me.team.is_none() || other.team != me.team,
    }
}

fn manhattan(a: Pos, b: Pos) -> u32 {
    u32::from(a.x.abs_diff(b.x) + a.y.abs_diff(b.y))
}

/// How close the chaser may come before a runner runs.
const FLEE_DISTANCE: u32 = 5;

/// A runner keeps away from the chaser: the way to the nearest square that is farther from it and
/// not a dead end, or `None` when the chaser is not close.
fn flee_chaser(obs: &Observation, me: &Participant, req: &PathRequest) -> Option<Vec<Dir>> {
    let chaser = obs
        .players
        .iter()
        .find(|p| p.alive && p.role == Role::Chaser)?;
    let now = manhattan(me.pos, chaser.pos);
    if now > FLEE_DISTANCE {
        return None;
    }
    let far = (now + 3).min(9);
    find_path(req, &|t| manhattan(t, chaser.pos) >= far && roomy(obs, t))
        .or_else(|| find_path(req, &|t| manhattan(t, chaser.pos) > now))
        .filter(|p| !p.is_empty())
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
    // with friendly fire a bomb that reaches a teammate kills the teammate
    if obs.config.mode == Mode::Team
        && obs.config.friendly_fire
        && obs
            .players
            .iter()
            .any(|p| p.alive && p.id != me.id && !is_enemy(me, p) && hit.tiles.contains(&p.pos))
    {
        return false;
    }
    // the chaser also bombs to corner a runner: one that is close cannot just run past the bomb
    let pressing = me.role == Role::Chaser
        && obs
            .players
            .iter()
            .any(|p| is_enemy(me, p) && manhattan(p.pos, me.pos) <= 3);
    let useful = pressing
        || !hit.destroyed.is_empty()
        || obs
            .players
            .iter()
            .any(|p| is_enemy(me, p) && hit.tiles.contains(&p.pos));
    if !useful {
        return false;
    }
    // The bomb is placed during the next tick and explodes `fuse_ticks` ticks later.
    let danger = DangerMap::with_bomb(obs, me.pos, me.placed_range(), obs.config.fuse_ticks + 1);
    let mut req = PathRequest::new(obs, &danger);
    req.earliest_move = me.cooldown_left.max(2);
    find_path(&req, &|_| true).is_some()
}
