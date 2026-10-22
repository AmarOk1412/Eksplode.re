//! What to draw and in which order.

use bomberust_core::{Observation, PowerUpKind, Tile};

use crate::interp::Interp;

/// Drawing layers, back to front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Box,
    Flame,
    PowerUp,
    Bomb,
    Player,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sprite {
    Wall,
    Block,
    Flame,
    PowerUp(PowerUpKind),
    Bomb { ticks_left: u32 },
    Player { slot: u8 },
}

/// One thing to draw at a position (the centre of its tile, in tile units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub pos: (f32, f32),
    pub layer: Layer,
    pub sprite: Sprite,
    /// Vertical offset in tiles (power-ups bob up and down).
    pub lift: f32,
}

impl Item {
    /// Order key: layer first, then rows from top to bottom so lower sprites cover higher ones.
    pub fn key(&self) -> f32 {
        f32::from(self.layer as u8) * 1000.0 + self.pos.1
    }
}

fn item(pos: (f32, f32), layer: Layer, sprite: Sprite) -> Item {
    Item {
        pos,
        layer,
        sprite,
        lift: 0.0,
    }
}

/// Every object sorted by `(layer, y, x)`; the floor is drawn by the caller. `phase` drives the
/// bobbing of power-ups.
pub fn draw_list(obs: &Observation, interp: &Interp, alpha: f32, phase: f32) -> Vec<Item> {
    let (w, h) = (obs.arena.width(), obs.arena.height());
    let mut objects = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let pos = (f32::from(x) + 0.5, f32::from(y) + 0.5);
            match obs.arena.tile(bomberust_core::Pos::new(x, y)) {
                Tile::Wall => objects.push(item(pos, Layer::Box, Sprite::Wall)),
                Tile::Block => objects.push(item(pos, Layer::Box, Sprite::Block)),
                Tile::Floor => {}
            }
        }
    }
    let centre = |x: u8, y: u8| (f32::from(x) + 0.5, f32::from(y) + 0.5);
    for f in &obs.flames {
        objects.push(item(centre(f.pos.x, f.pos.y), Layer::Flame, Sprite::Flame));
    }
    for (i, u) in obs.powerups.iter().enumerate() {
        let mut it = item(
            centre(u.pos.x, u.pos.y),
            Layer::PowerUp,
            Sprite::PowerUp(u.kind),
        );
        it.lift = 0.05 * (phase * 0.3 + i as f32).sin();
        objects.push(it);
    }
    for b in &obs.bombs {
        let sprite = Sprite::Bomb {
            ticks_left: b.ticks_left,
        };
        objects.push(item(centre(b.pos.x, b.pos.y), Layer::Bomb, sprite));
    }
    for (slot, p) in obs.players.iter().enumerate() {
        if p.alive {
            let (x, y) = interp.position(slot, alpha);
            let sprite = Sprite::Player { slot: slot as u8 };
            objects.push(item((x + 0.5, y + 0.5), Layer::Player, sprite));
        }
    }
    objects.sort_by(|a, b| {
        let by_x = a.pos.0.partial_cmp(&b.pos.0);
        a.key()
            .partial_cmp(&b.key())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(by_x.unwrap_or(std::cmp::Ordering::Equal))
    });
    objects
}
