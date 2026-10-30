//! The default bot: flee fire, grab power-ups, hunt opponents, break blocks.

use eksplode_re_core::closing::closing_order;
use eksplode_re_core::{
    blast, Dir, Mode, Observation, Participant, Phase, PlayerInput, Pos, Rng, Role,
};

use crate::danger::DangerMap;
use crate::pathing::{find_path, has_malus, passable, PathRequest};
use crate::profile::{Behaviour, Profile, Skill};
use crate::Bot;

/// Chance per tick to take a random safe step when nothing else is worth doing.
const WANDER_CHANCE: f32 = 0.3;

/// A deterministic bot; the seed only breaks ties between equally good moves.
pub struct DefaultBot {
    rng: Rng,
    profile: Profile,
    skill: Skill,
    /// The input of the last decision, without bomb or punch.
    last: PlayerInput,
    /// Ticks left before the next decision.
    waiting: u32,
}

impl DefaultBot {
    /// Balanced behaviour at Hard difficulty, the bot from before behaviours existed.
    pub fn new(seed: u64) -> Self {
        Self::with_profile(seed, Profile::default())
    }

    /// A bot with this behaviour and difficulty.
    pub fn with_profile(seed: u64, profile: Profile) -> Self {
        Self {
            rng: Rng::new(seed),
            profile,
            skill: Skill::of(profile.difficulty),
            last: PlayerInput::default(),
            waiting: 0,
        }
    }

    /// Its behaviour and difficulty.
    pub fn profile(&self) -> Profile {
        self.profile
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
        if self.waiting > 0 {
            self.waiting -= 1;
            return self.held(obs, me);
        }
        self.waiting = self.skill.think_every.saturating_sub(1);
        let late;
        let view = if self.skill.react_ticks > 0 {
            late = self.late_view(obs);
            &late
        } else {
            obs
        };
        let planned = self.plan(view);
        let input = self.mistake(view, planned);
        self.last = PlayerInput {
            bomb: false,
            punch: false,
            ..input
        };
        input
    }
}

impl DefaultBot {
    /// Between two decisions: keep walking the same way unless that square burns or is blocked.
    fn held(&self, obs: &Observation, me: &Participant) -> PlayerInput {
        let mv = self.last.mv.filter(|d| {
            me.pos
                .step(*d, obs.arena.width(), obs.arena.height())
                .is_some_and(|t| passable(obs, t) && !obs.flames.iter().any(|f| f.pos == t))
        });
        PlayerInput {
            mv,
            ..PlayerInput::default()
        }
    }

    /// What a slow bot has noticed: opponents' bombs younger than its reaction time are missing.
    fn late_view(&self, obs: &Observation) -> Observation {
        let mut view = obs.clone();
        let fuse = obs.config.fuse_ticks;
        view.bombs.retain(|b| {
            b.owner == obs.me || fuse.saturating_sub(b.ticks_left) >= self.skill.react_ticks
        });
        view
    }

    /// Now and then a bot below Hard gets it wrong: it skips a bomb, hesitates or takes a wrong
    /// turn, but never onto a burning square.
    fn mistake(&mut self, obs: &Observation, planned: PlayerInput) -> PlayerInput {
        if self.skill.mistake_chance <= 0.0 || !self.rng.chance(self.skill.mistake_chance) {
            return planned;
        }
        if planned.bomb {
            return PlayerInput::default();
        }
        let me = &obs.players[obs.me.index()];
        let in_danger = !DangerMap::from_observation(obs).safe_from(me.pos, 1);
        if in_danger || self.rng.chance(0.5) {
            return PlayerInput::default();
        }
        for d in self.shuffled_dirs() {
            let Some(t) = me.pos.step(d, obs.arena.width(), obs.arena.height()) else {
                continue;
            };
            if passable(obs, t) && !obs.flames.iter().any(|f| f.pos == t) {
                return PlayerInput {
                    mv: Some(d),
                    ..PlayerInput::default()
                };
            }
        }
        PlayerInput::default()
    }

    /// The best input for what the bot sees.
    fn plan(&mut self, obs: &Observation) -> PlayerInput {
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
            let flee_at = match self.profile.behaviour {
                Behaviour::Protective => FLEE_DISTANCE + 2,
                _ => FLEE_DISTANCE,
            };
            if let Some(i) = step_to(flee_chaser(obs, me, &req, flee_at)) {
                return i;
            }
        }
        if let Some(progress) = closing_progress(obs) {
            req.max_depth = 14;
            if let Some(i) = step_to(move_inward(obs, me, &req, progress)) {
                return i;
            }
        }
        let threats: Vec<Pos> = obs
            .players
            .iter()
            .filter(|p| is_enemy(p, me))
            .map(|p| p.pos)
            .collect();
        if self.profile.behaviour == Behaviour::Protective {
            if let Some(i) = keep_away(obs, me, &danger, &threats, req.order) {
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

        let powerup = |t: Pos| {
            obs.powerups
                .iter()
                .any(|u| u.pos == t && !u.kind.is_malus())
        };
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
        let dig = |t: Pos| {
            t != me.pos
                && roomy(obs, t)
                && !blast(&obs.arena, t, me.placed_range()).destroyed.is_empty()
        };
        let approach = |t: Pos| enemies.iter().any(|e| manhattan(t, *e) == 1);
        let has_bomb = me.bombs_active < me.bombs_max;
        if self.profile.behaviour == Behaviour::Protective {
            req.avoid = threats
                .iter()
                .flat_map(|t| {
                    std::iter::once(*t).chain(
                        Dir::ALL
                            .iter()
                            .filter_map(|d| t.step(*d, obs.arena.width(), obs.arena.height())),
                    )
                })
                .filter(|p| *p != me.pos)
                .collect();
        }
        for goal in plan(self.profile.behaviour) {
            let path = match *goal {
                Goal::PowerUp(depth) => {
                    req.max_depth = depth;
                    find_path(&req, &powerup)
                }
                Goal::Hunt(depth) if has_bomb => {
                    req.max_depth = depth;
                    find_path(&req, &hunt)
                }
                Goal::Dig(depth) if has_bomb => {
                    req.max_depth = depth;
                    find_path(&req, &dig)
                }
                Goal::Approach(depth) => {
                    req.max_depth = depth;
                    find_path(&req, &approach)
                }
                Goal::KeepAway => {
                    if let Some(i) = keep_away(obs, me, &danger, &threats, req.order) {
                        return i;
                    }
                    None
                }
                Goal::Hunt(_) | Goal::Dig(_) => None,
            };
            if let Some(i) = step_to(path) {
                return i;
            }
        }
        self.wander(obs, me, &danger)
    }
}

/// Something worth walking to once the bot is safe.
#[derive(Clone, Copy)]
enum Goal {
    /// A bonus within this many steps.
    PowerUp(u32),
    /// A square from which a bomb reaches an opponent.
    Hunt(u32),
    /// A square from which a bomb destroys a block.
    Dig(u32),
    /// A square next to an opponent.
    Approach(u32),
    /// A step away from a close opponent.
    KeepAway,
}

/// The goals of a behaviour, most wanted first.
fn plan(behaviour: Behaviour) -> &'static [Goal] {
    match behaviour {
        Behaviour::Balanced => &[Goal::PowerUp(8), Goal::Hunt(14), Goal::Dig(14)],
        Behaviour::Aggressive => &[
            Goal::Hunt(20),
            Goal::PowerUp(4),
            Goal::Dig(14),
            Goal::Approach(20),
        ],
        Behaviour::Protective => &[
            Goal::PowerUp(8),
            Goal::Dig(14),
            Goal::Hunt(5),
            Goal::KeepAway,
        ],
    }
}

/// How close an opponent may come before a Protective bot steps back.
const KEEP_AWAY: u32 = 3;

/// A safe step that gets farther from the nearest threat, when one is within [`KEEP_AWAY`].
fn keep_away(
    obs: &Observation,
    me: &Participant,
    danger: &DangerMap,
    threats: &[Pos],
    order: [Dir; 4],
) -> Option<PlayerInput> {
    let distance = |p: Pos| threats.iter().map(|t| manhattan(p, *t)).min();
    let now = distance(me.pos)?;
    if now > KEEP_AWAY {
        return None;
    }
    order
        .into_iter()
        .filter_map(|d| {
            let t = me.pos.step(d, obs.arena.width(), obs.arena.height())?;
            let far = distance(t)?;
            (passable(obs, t) && !has_malus(obs, t) && danger.safe_from(t, 1) && far > now)
                .then_some((far, d))
        })
        .max_by_key(|(far, _)| *far)
        .map(|(_, d)| PlayerInput {
            mv: Some(d),
            ..PlayerInput::default()
        })
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
fn flee_chaser(
    obs: &Observation,
    me: &Participant,
    req: &PathRequest,
    flee_at: u32,
) -> Option<Vec<Dir>> {
    let chaser = obs
        .players
        .iter()
        .find(|p| p.alive && p.role == Role::Chaser)?;
    let now = manhattan(me.pos, chaser.pos);
    if now > flee_at {
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
