//! Smooth player movement between two server ticks.

use eksplode_re_core::{Dir, Event, Observation};

use crate::look::Frame;

/// How many ticks the dying animation of a player lasts.
pub const DYING_TICKS: f32 = 16.0;
pub const PUNCH_TICKS: f32 = 7.0;

/// Remembers where every player was one tick ago and now.
#[derive(Clone, Debug, Default)]
pub struct Interp {
    tick: Option<u32>,
    prev: [(f32, f32); 4],
    cur: [(f32, f32); 4],
    /// The tick of each player's latest step, and how many steps they took.
    last_move: [Option<u32>; 4],
    steps: [u32; 4],
    /// Ticks between two steps of each player (their speed).
    interval: [u32; 4],
    /// Who was alive in the latest snapshot, and the tick each player was seen dying.
    alive: [bool; 4],
    died_at: [Option<u32>; 4],
    kart_stints: [Option<u32>; 4],
    punched_at: [Option<(u32, Dir)>; 4],
}

impl Interp {
    /// Feeds a new snapshot. A snapshot of the same tick only refreshes the current positions;
    /// an older tick (a new match) jumps without sliding.
    pub fn update(&mut self, obs: &Observation) {
        let mut now = [(0.0, 0.0); 4];
        let mut alive = [false; 4];
        for (slot, p) in obs.players.iter().enumerate().take(4) {
            now[slot] = p
                .kart
                .and_then(|kart| {
                    eksplode_re_core::kart::view_position(
                        obs.arena.width(),
                        obs.arena.height(),
                        kart.index,
                    )
                })
                .map(|(column, row)| (column - 0.5, row - 0.5))
                .unwrap_or((f32::from(p.pos.x), f32::from(p.pos.y)));
            alive[slot] = p.alive;
        }
        for (slot, p) in obs.players.iter().enumerate().take(4) {
            self.interval[slot] = p.effective_cooldown().max(1);
        }
        match self.tick {
            Some(t) if obs.tick == t => {}
            Some(t) if obs.tick > t => {
                self.prev = self.cur;
                for (slot, here) in now.iter().enumerate() {
                    if *here != self.cur[slot] {
                        self.last_move[slot] = Some(obs.tick);
                        self.steps[slot] += 1;
                    }
                    if self.alive[slot] && !alive[slot] {
                        self.died_at[slot] = Some(obs.tick);
                    } else if alive[slot] {
                        self.died_at[slot] = None;
                    }
                }
            }
            _ => {
                self.prev = now;
                self.last_move = [None; 4];
                self.steps = [0; 4];
                self.died_at = [None; 4];
                self.punched_at = [None; 4];
            }
        }
        for (slot, player) in obs.players.iter().enumerate() {
            let stint = player.kart.map(|kart| kart.entered_tick);
            if stint != self.kart_stints[slot]
                || (player.alive && !self.alive[slot])
                || self
                    .tick
                    .is_some_and(|tick| obs.tick > tick.saturating_add(1))
            {
                self.prev[slot] = now[slot];
            }
            if stint.is_some() {
                self.died_at[slot] = None;
            }
            self.kart_stints[slot] = stint;
        }
        self.cur = now;
        self.alive = alive;
        self.tick = Some(obs.tick);
    }

    pub fn update_with_events(&mut self, obs: &Observation, events: &[Event]) {
        let fresh = self.tick != Some(obs.tick);
        self.update(obs);
        if !fresh {
            return;
        }
        for event in events {
            if let Event::BombPunched { player, dir, .. } | Event::BombKicked { player, dir, .. } =
                event
            {
                if let Some(cue) = self.punched_at.get_mut(player.index()) {
                    *cue = Some((obs.tick, *dir));
                }
            }
        }
    }

    pub fn punch(&self, slot: usize, ticks: f32) -> Option<(Dir, f32)> {
        let (at, direction) = (*self.punched_at.get(slot)?)?;
        if !self.alive[slot] {
            return None;
        }
        let ticks = if ticks.is_finite() {
            ticks.max(0.0)
        } else {
            0.0
        };
        let elapsed = self.tick?.saturating_sub(at) as f32 + ticks;
        (elapsed < PUNCH_TICKS).then_some((direction, elapsed / PUNCH_TICKS))
    }

    /// How far (0.0-1.0) a player who just died is into the dying animation, `ticks` after the
    /// latest snapshot (more than one tick once the snapshots stop at the end of a match);
    /// `None` for the living and once the animation is over.
    pub fn dying(&self, slot: usize, ticks: f32) -> Option<f32> {
        let at = (*self.died_at.get(slot)?)?;
        let ticks = if ticks.is_finite() {
            ticks.max(0.0)
        } else {
            0.0
        };
        let elapsed = self.tick?.saturating_sub(at) as f32 + ticks;
        (elapsed < DYING_TICKS).then_some(elapsed / DYING_TICKS)
    }

    /// Tile coordinates of a player; `alpha` 0.0 is the previous tick, 1.0 the current one.
    pub fn position(&self, slot: usize, alpha: f32) -> (f32, f32) {
        let (Some(a), Some(b)) = (self.prev.get(slot), self.cur.get(slot)) else {
            return (0.0, 0.0);
        };
        let t = if alpha.is_nan() {
            0.0
        } else {
            alpha.clamp(0.0, 1.0)
        };
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }

    /// The pose of a player `alpha` of the way to the next tick: a step frame for the first half
    /// of the interval after each step (the foot alternates with every step), standing in the
    /// second half and as soon as the player has not stepped for a while.
    pub fn pose(&self, slot: usize, alpha: f32) -> Frame {
        let (Some(Some(at)), Some(tick), Some(interval)) =
            (self.last_move.get(slot), self.tick, self.interval.get(slot))
        else {
            return Frame::Idle;
        };
        let alpha = if alpha.is_nan() {
            0.0
        } else {
            alpha.clamp(0.0, 1.0)
        };
        let interval = *interval as f32;
        let elapsed = tick.saturating_sub(*at) as f32 + alpha;
        if elapsed > interval + 1.5 || elapsed / interval >= 0.5 {
            Frame::Idle
        } else if self.steps[slot] % 2 == 1 {
            Frame::StepA
        } else {
            Frame::StepB
        }
    }
}
