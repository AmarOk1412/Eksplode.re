//! Smooth player movement between two server ticks.

use bomberust_core::Observation;

/// Remembers where every player was one tick ago and now.
#[derive(Clone, Debug, Default)]
pub struct Interp {
    tick: Option<u32>,
    prev: [(f32, f32); 4],
    cur: [(f32, f32); 4],
}

impl Interp {
    /// Feeds a new snapshot. A snapshot of the same tick only refreshes the current positions;
    /// an older tick (a new match) jumps without sliding.
    pub fn update(&mut self, obs: &Observation) {
        let mut now = [(0.0, 0.0); 4];
        for (slot, p) in obs.players.iter().enumerate() {
            now[slot] = (f32::from(p.pos.x), f32::from(p.pos.y));
        }
        match self.tick {
            Some(t) if obs.tick == t => {}
            Some(t) if obs.tick > t => self.prev = self.cur,
            _ => self.prev = now,
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
}
