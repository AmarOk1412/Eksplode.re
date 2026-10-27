//! What to draw and in which order.

use eksplode_re_core::kick::predict_slide_stop;
use eksplode_re_core::{Observation, PowerUpKind, Tile};

use crate::interp::Interp;
use crate::look::{Frame, View};

/// Drawing layers, back to front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Box,
    /// The announcement of a falling block: on the floor, under everything that stands on it.
    Warning,
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
    /// A block that lands in `ticks_left` ticks; `lead` is the whole countdown.
    Falling {
        ticks_left: u32,
        lead: u32,
    },
    PowerUp(PowerUpKind),
    /// The crown and the hearts above the chaser; `hidden` while the hearts blink.
    ChaserMark {
        hearts: u8,
        hidden: bool,
    },
    /// The ring under a player of team `0` or `1`.
    TeamRing {
        team: u8,
    },
    /// `flying` bombs are in the air after a punch and cast a shadow on the ground.
    Bomb {
        ticks_left: u32,
        flying: bool,
    },
    Player {
        slot: u8,
    },
}

/// One thing to draw at a position (the centre of its tile, in tile units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub pos: (f32, f32),
    pub layer: Layer,
    pub sprite: Sprite,
    /// Vertical offset in tiles, negative is up (power-ups bob, punched bombs arc).
    pub lift: f32,
    /// The malus that tints a player.
    pub malus: Option<PowerUpKind>,
    /// Which way a player looks and which step of the walk they are in.
    pub view: View,
    pub frame: Frame,
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
        malus: None,
        view: View::Down,
        frame: Frame::Idle,
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
            match obs.arena.tile(eksplode_re_core::Pos::new(x, y)) {
                Tile::Wall => objects.push(item(pos, Layer::Box, Sprite::Wall)),
                Tile::Block => objects.push(item(pos, Layer::Box, Sprite::Block)),
                Tile::Floor => {}
            }
        }
    }
    let centre = |x: u8, y: u8| (f32::from(x) + 0.5, f32::from(y) + 0.5);
    for f in &obs.falling {
        let sprite = Sprite::Falling {
            ticks_left: f.ticks_left,
            lead: obs.config.closing_lead_ticks,
        };
        objects.push(item(centre(f.pos.x, f.pos.y), Layer::Warning, sprite));
    }
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
    for (i, b) in obs.bombs.iter().enumerate() {
        let mut sprite = Sprite::Bomb {
            ticks_left: b.ticks_left,
            flying: false,
        };
        let mut at = centre(b.pos.x, b.pos.y);
        let mut lift = 0.0;
        if let Some(slide) = b.slide {
            // a kicked bomb rolls on towards the next square, but only while there is room
            let ahead = predict_slide_stop(&obs.arena, &obs.players, &obs.bombs, i) != b.pos;
            if ahead {
                let every = obs.config.slide_ticks.max(1) as f32;
                let a = if alpha.is_finite() {
                    alpha.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let progress = ((every - slide.step_left as f32 + a) / every).clamp(0.0, 1.0);
                let (dx, dy) = slide.dir.delta();
                at = (
                    at.0 + f32::from(dx) * progress,
                    at.1 + f32::from(dy) * progress,
                );
            }
        }
        if let Some(f) = b.flight {
            // the bomb hops one square per `punch_hop_ticks`: half a hop in, it is half a tile
            // along and at the top of its arc
            let hop = obs.config.punch_hop_ticks.max(1) as f32;
            let progress = (1.0 - f.hop_left as f32 / hop).clamp(0.0, 1.0);
            let (dx, dy) = f.dir.delta();
            at = (
                at.0 + f32::from(dx) * progress,
                at.1 + f32::from(dy) * progress,
            );
            lift = -(std::f32::consts::PI * progress).sin() * 0.5;
            sprite = Sprite::Bomb {
                ticks_left: b.ticks_left,
                flying: true,
            };
        }
        let mut it = item(at, Layer::Bomb, sprite);
        it.lift = lift;
        objects.push(it);
    }
    for (slot, p) in obs.players.iter().enumerate() {
        if p.alive {
            let (x, y) = interp.position(slot, alpha);
            if let Some(team) = p.team {
                // just above the row of the player so the ring is drawn first
                let ring = Sprite::TeamRing { team };
                objects.push(item((x + 0.5, y + 0.5 - 0.01), Layer::Player, ring));
            }
            if p.role == eksplode_re_core::Role::Chaser {
                // drawn after the player of the same row
                let mark = Sprite::ChaserMark {
                    hearts: p.hearts,
                    hidden: p.shield_left > 0 && (obs.tick / 2) % 2 == 0,
                };
                objects.push(item((x + 0.5, y + 0.5 + 0.01), Layer::Player, mark));
            }
            let sprite = Sprite::Player { slot: slot as u8 };
            let mut it = item((x + 0.5, y + 0.5), Layer::Player, sprite);
            it.malus = p.malus.map(|m| m.kind);
            it.view = View::from_dir(p.facing);
            it.frame = interp.pose(slot, alpha);
            objects.push(it);
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
