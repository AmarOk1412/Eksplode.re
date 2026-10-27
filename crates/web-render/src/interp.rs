//! Smooth player movement between two server ticks.

use eksplode_re_core::Observation;

use crate::look::Frame;

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
}

impl Interp {
    /// Feeds a new snapshot. A snapshot of the same tick only refreshes the current positions;
    /// an older tick (a new match) jumps without sliding.
    pub fn update(&mut self, obs: &Observation) {
        let mut now = [(0.0, 0.0); 4];
        for (slot, p) in obs.players.iter().enumerate() {
            now[slot] = (f32::from(p.pos.x), f32::from(p.pos.y));
        }
        for (slot, p) in obs.players.iter().enumerate() {
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
                }
            }
            _ => {
                self.prev = now;
                self.last_move = [None; 4];
                self.steps = [0; 4];
            }
        }
        self.cur = now;
        self.tick = Some(obs.tick);
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
