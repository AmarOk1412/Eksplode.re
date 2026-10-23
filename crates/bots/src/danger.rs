//! When and for how long each tile is going to burn, derived from the public observation.

use bomberust_core::punch::predict_landing;
use bomberust_core::{blast, Observation, Pos};

/// Burning windows per tile, in steps counted from the observation (step 1 is the next tick).
#[derive(Clone, Debug)]
pub struct DangerMap {
    width: u8,
    height: u8,
    /// Inclusive `(first, last)` step ranges during which the tile is lethal.
    windows: Vec<Vec<(u32, u32)>>,
}

impl DangerMap {
    /// Danger from the bombs and flames that exist now, including chain reactions.
    pub fn from_observation(obs: &Observation) -> Self {
        Self::build(obs, None)
    }

    /// Like [`DangerMap::from_observation`] plus an extra bomb that explodes at step `explodes_at`.
    pub fn with_bomb(obs: &Observation, pos: Pos, range: u8, explodes_at: u32) -> Self {
        Self::build(obs, Some((pos, range, explodes_at)))
    }

    fn build(obs: &Observation, extra: Option<(Pos, u8, u32)>) -> Self {
        let mut bombs: Vec<(Pos, u8, u32)> = Vec::new();
        for (i, b) in obs.bombs.iter().enumerate() {
            bombs.push((b.pos, b.range, b.ticks_left));
            if b.flight.is_some() {
                // a bomb in the air explodes where it lands, or where it is if the fuse is
                // shorter than the flight: both are dangerous
                let landing = predict_landing(&obs.arena, &obs.players, &obs.bombs, i);
                if landing != b.pos {
                    bombs.push((landing, b.range, b.ticks_left));
                }
            }
        }
        bombs.extend(extra);
        let blasts: Vec<_> = bombs
            .iter()
            .map(|(p, r, _)| blast(&obs.arena, *p, *r))
            .collect();

        // A bomb reached by an earlier blast goes off with it.
        let mut at: Vec<u32> = bombs.iter().map(|b| b.2).collect();
        loop {
            let mut changed = false;
            for a in 0..bombs.len() {
                for b in 0..bombs.len() {
                    if a != b && at[a] < at[b] && blasts[a].tiles.contains(&bombs[b].0) {
                        at[b] = at[a];
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let (width, height) = (obs.arena.width(), obs.arena.height());
        let mut map = DangerMap {
            width,
            height,
            windows: vec![Vec::new(); usize::from(width) * usize::from(height)],
        };
        let burn = obs.config.flame_ticks.max(1);
        for (i, b) in blasts.iter().enumerate() {
            for p in &b.tiles {
                map.add(*p, at[i], at[i] + burn - 1);
            }
        }
        for f in &obs.flames {
            if f.ticks_left > 1 {
                map.add(f.pos, 1, f.ticks_left - 1);
            }
        }
        map
    }

    fn add(&mut self, p: Pos, first: u32, last: u32) {
        if let Some(i) = self.index(p) {
            self.windows[i].push((first, last));
        }
    }

    fn index(&self, p: Pos) -> Option<usize> {
        (p.x < self.width && p.y < self.height)
            .then(|| usize::from(p.y) * usize::from(self.width) + usize::from(p.x))
    }

    fn at(&self, p: Pos) -> &[(u32, u32)] {
        self.index(p).map_or(&[], |i| &self.windows[i])
    }

    /// Earliest step at which the tile burns.
    pub fn first_danger(&self, p: Pos) -> Option<u32> {
        self.at(p).iter().map(|w| w.0).min()
    }

    /// Does the tile burn at any step in `from..=to`?
    pub fn dangerous_during(&self, p: Pos, from: u32, to: u32) -> bool {
        from <= to && self.at(p).iter().any(|w| w.0 <= to && w.1 >= from)
    }

    /// Is the tile free of fire at every step from `from` onwards?
    pub fn safe_from(&self, p: Pos, from: u32) -> bool {
        self.at(p).iter().all(|w| w.1 < from)
    }
}
