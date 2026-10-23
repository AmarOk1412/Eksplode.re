//! The tile grid and its generation.

use crate::action::Pos;
use crate::config::Config;
use crate::error::CoreError;
use crate::rng::Rng;
use crate::room::Mode;

/// One cell of the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Tile {
    Floor,
    /// Indestructible.
    Wall,
    /// Destructible; may hide a power-up.
    Block,
}

/// A rectangular grid of tiles, stored row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Arena {
    width: u8,
    height: u8,
    tiles: Vec<Tile>,
}

impl Arena {
    /// Builds an arena from text rows: `#` wall, `.` floor, `+` block.
    pub fn from_rows(rows: &[&str]) -> Result<Arena, CoreError> {
        let bad = |m: &str| Err(CoreError::InvalidConfig(m.to_string()));
        let Some(first) = rows.first() else {
            return bad("arena has no rows");
        };
        let width = first.chars().count();
        if width == 0 || width > 255 || rows.len() > 255 {
            return bad("arena size must be in 1..=255");
        }
        let mut tiles = Vec::with_capacity(width * rows.len());
        for row in rows {
            if row.chars().count() != width {
                return bad("arena rows have different lengths");
            }
            for c in row.chars() {
                tiles.push(match c {
                    '#' => Tile::Wall,
                    '.' => Tile::Floor,
                    '+' => Tile::Block,
                    _ => return bad("arena rows may only contain '#', '.' and '+'"),
                });
            }
        }
        Ok(Arena {
            width: width as u8,
            height: rows.len() as u8,
            tiles,
        })
    }

    /// Generates the standard arena: border and fixed pillar walls, random blocks, clear corners.
    pub fn generate(config: &Config, rng: &mut Rng) -> Arena {
        let (w, h) = (config.width, config.height);
        let mut arena = Arena {
            width: w,
            height: h,
            tiles: vec![Tile::Floor; usize::from(w) * usize::from(h)],
        };
        let chaser = config.mode == Mode::Chaser;
        let mut clear: Vec<Pos> = start_areas(w, h);
        if chaser {
            // the chaser starts in the middle: the middle and the squares around it are free
            let (cx, cy) = (w / 2, h / 2);
            clear.extend([
                Pos::new(cx, cy),
                Pos::new(cx - 1, cy),
                Pos::new(cx + 1, cy),
                Pos::new(cx, cy - 1),
                Pos::new(cx, cy + 1),
            ]);
        }
        let (density, keep_every) = if chaser {
            (
                config.chaser_block_density,
                usize::from(config.chaser_pillar_mod.max(1)),
            )
        } else {
            (config.block_density, 1)
        };
        for y in 0..h {
            for x in 0..w {
                let p = Pos::new(x, y);
                let border = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                // Normal: every even/even square is a pillar; Chaser: about one in `keep_every`
                let pillar = x % 2 == 0
                    && y % 2 == 0
                    && (usize::from(x / 2) + usize::from(y / 2)) % keep_every == 0
                    && !(chaser && clear.contains(&p));
                if border || pillar {
                    arena.set_tile(p, Tile::Wall);
                } else if !clear.contains(&p) && rng.chance(density) {
                    arena.set_tile(p, Tile::Block);
                }
            }
        }
        arena
    }

    /// Does the tile list match the declared size? Deserialised data can be inconsistent.
    pub fn is_valid(&self) -> bool {
        self.tiles.len() == usize::from(self.width) * usize::from(self.height)
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    /// Tile at `p`; anything outside the grid reads as a wall.
    pub fn tile(&self, p: Pos) -> Tile {
        if p.x >= self.width || p.y >= self.height {
            return Tile::Wall;
        }
        self.tiles[self.index(p)]
    }

    /// Replaces a tile; positions outside the grid are ignored.
    pub fn set_tile(&mut self, p: Pos, tile: Tile) {
        if p.x < self.width && p.y < self.height {
            let i = self.index(p);
            self.tiles[i] = tile;
        }
    }

    /// All positions holding `tile`, in row-major order.
    pub fn positions_of(&self, tile: Tile) -> Vec<Pos> {
        let mut out = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let p = Pos::new(x, y);
                if self.tile(p) == tile {
                    out.push(p);
                }
            }
        }
        out
    }

    /// The four corner start tiles for slots 0..4.
    pub fn start_positions(&self) -> [Pos; 4] {
        corners(self.width, self.height)
    }

    fn index(&self, p: Pos) -> usize {
        usize::from(p.y) * usize::from(self.width) + usize::from(p.x)
    }
}

fn corners(w: u8, h: u8) -> [Pos; 4] {
    [
        Pos::new(1, 1),
        Pos::new(w - 2, 1),
        Pos::new(1, h - 2),
        Pos::new(w - 2, h - 2),
    ]
}

/// Each corner plus its two neighbours.
fn start_areas(w: u8, h: u8) -> Vec<Pos> {
    let mut out = Vec::new();
    for c in corners(w, h) {
        let dx: i16 = if c.x == 1 { 1 } else { -1 };
        let dy: i16 = if c.y == 1 { 1 } else { -1 };
        out.push(c);
        out.push(Pos::new((i16::from(c.x) + dx) as u8, c.y));
        out.push(Pos::new(c.x, (i16::from(c.y) + dy) as u8));
    }
    out
}
