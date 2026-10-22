//! Boxes, cubes and the colour palette.

use crate::iso::project;

pub const FACE_TOP: f32 = 1.0;
pub const FACE_LEFT: f32 = 0.8;
pub const FACE_RIGHT: f32 = 0.6;

pub const WALL_TOP: [f32; 3] = [0.58, 0.60, 0.66];
pub const WALL_LEFT: [f32; 3] = [0.46, 0.48, 0.54];
pub const WALL_RIGHT: [f32; 3] = [0.35, 0.37, 0.42];
pub const BLOCK_TOP: [f32; 3] = [0.80, 0.62, 0.38];
pub const BLOCK_LEFT: [f32; 3] = [0.64, 0.48, 0.28];
pub const BLOCK_RIGHT: [f32; 3] = [0.50, 0.36, 0.20];
pub const FLOOR_A: [f32; 3] = [0.17, 0.20, 0.24];
pub const FLOOR_B: [f32; 3] = [0.20, 0.23, 0.28];

fn hex(r: u8, g: u8, b: u8) -> [f32; 3] {
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ]
}

/// Colour of a player slot: red, blue, green, yellow.
pub fn player_color(slot: u8) -> [f32; 3] {
    match slot % 4 {
        0 => hex(0xdc, 0x32, 0x2f),
        1 => hex(0x26, 0x8b, 0xd2),
        2 => hex(0x3c, 0xb4, 0x4b),
        _ => hex(0xe6, 0xbe, 0x1e),
    }
}

/// Multiplies the brightness of a colour.
pub fn shade(c: [f32; 3], factor: f32) -> [f32; 3] {
    [c[0] * factor, c[1] * factor, c[2] * factor]
}

/// The digit drawn on a player's cube: the slot number starting at 1.
pub fn badge_char(slot: u8) -> char {
    char::from(b'1' + slot % 4)
}

/// The three visible faces of a box in view space, each as four points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Faces {
    pub top: [(f32, f32); 4],
    pub left: [(f32, f32); 4],
    pub right: [(f32, f32); 4],
}

/// A box centred on the tile position `(cx, cy)`, `footprint` tiles wide, from height `z0` up by `h`.
pub fn box_faces(cx: f32, cy: f32, footprint: f32, z0: f32, h: f32) -> Faces {
    let half = footprint / 2.0;
    let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
    let zt = z0 + h;
    Faces {
        top: [
            project(x0, y0, zt),
            project(x1, y0, zt),
            project(x1, y1, zt),
            project(x0, y1, zt),
        ],
        left: [
            project(x0, y1, z0),
            project(x1, y1, z0),
            project(x1, y1, zt),
            project(x0, y1, zt),
        ],
        right: [
            project(x1, y1, z0),
            project(x1, y0, z0),
            project(x1, y0, zt),
            project(x1, y1, zt),
        ],
    }
}
