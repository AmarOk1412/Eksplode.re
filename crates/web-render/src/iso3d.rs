//! A semi-3D character in an isometric view, for the lobby cards.
//!
//! The character is built from boxes (feet, arms, body, belt, head, accessories) and drawn with
//! the painter's algorithm: boxes are added from back to front and every face is a flat quad.
//! The projection is 2:1 isometric; the faces of a box are shaded 1.0 on top, 0.8 on the left
//! and 0.6 on the right. The pixel face sits on the front-right face of the head and turns to
//! the front-left face every [`YAW_PERIOD`] seconds; the whole figure bobs with [`BOB_PERIOD`].

use crate::commands::{Command, Kind};
use crate::look::{
    skin, Face, Gender, Head, Look, CAP, CROWN, CUP, FRAME, HAT, HAT_BAND, HEADBAND, MASK,
    MOUSTACHE, SUNGLASSES,
};
use crate::sprites::{mix, Rgb};

pub const SHADE_TOP: f32 = 1.0;
pub const SHADE_LEFT: f32 = 0.8;
pub const SHADE_RIGHT: f32 = 0.6;
/// How far the figure rises while bobbing, as a share of the picture height.
pub const BOB_AMPLITUDE: f32 = 0.03;
/// Seconds per bob.
pub const BOB_PERIOD: f32 = 2.0;
/// Seconds between two turns of the head.
pub const YAW_PERIOD: f32 = 6.0;

type P3 = [f32; 3];

/// Isometric projection of a point `(x, y, z)`: `x` runs down to the right, `y` down to the left
/// and `z` up; one step on the ground moves one unit sideways and half a unit down.
pub fn project(p: P3) -> (f32, f32) {
    (p[0] - p[1], (p[0] + p[1]) * 0.5 - p[2])
}

/// The ground point `(x, y)` that [`project`] maps to a screen point.
pub fn unproject_ground(s: (f32, f32)) -> (f32, f32) {
    (s.1 + s.0 * 0.5, s.1 - s.0 * 0.5)
}

/// Vertical offset in pixels (zero or negative, up) of the figure `phase` seconds in.
pub fn bob_offset(phase: f32, height: f32) -> f32 {
    if !phase.is_finite() {
        return 0.0;
    }
    let turn = 2.0 * std::f32::consts::PI * phase / BOB_PERIOD;
    -BOB_AMPLITUDE * height * 0.5 * (1.0 - turn.cos())
}

/// Does the face look to the front-left at `phase` seconds? It alternates every [`YAW_PERIOD`].
pub fn faces_left(phase: f32) -> bool {
    phase.is_finite() && (phase / YAW_PERIOD).floor().rem_euclid(2.0) == 1.0
}

/// A colour scaled by a shade.
pub fn shade(c: Rgb, s: f32) -> Rgb {
    [c[0] * s, c[1] * s, c[2] * s]
}

/// A parallelogram on a face of a box: `point(u, v)` with `u`, `v` in `0.0..=1.0`.
#[derive(Clone, Copy)]
struct Frame {
    origin: P3,
    u: P3,
    v: P3,
    shade: f32,
}

impl Frame {
    fn point(&self, u: f32, v: f32) -> P3 {
        [
            self.origin[0] + self.u[0] * u + self.v[0] * v,
            self.origin[1] + self.u[1] * u + self.v[1] * v,
            self.origin[2] + self.u[2] * u + self.v[2] * v,
        ]
    }
}

struct Poly {
    points: [P3; 4],
    color: [f32; 4],
    bobs: bool,
}

struct Scene {
    /// Turn the character a quarter so that its front is the left face.
    swap: bool,
    polys: Vec<Poly>,
}

impl Scene {
    fn push(&mut self, points: [P3; 4], c: Rgb, alpha: f32, bobs: bool) {
        self.polys.push(Poly {
            points,
            color: [c[0], c[1], c[2], alpha],
            bobs,
        });
    }

    /// A box of the character, modelled facing `+x`; returns its front face.
    fn cube(&mut self, x: (f32, f32), y: (f32, f32), z: (f32, f32), c: Rgb) -> Frame {
        let (x, y) = if self.swap { (y, x) } else { (x, y) };
        let (x0, x1, y0, y1, z0, z1) = (x.0, x.1, y.0, y.1, z.0, z.1);
        self.push(
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            shade(c, SHADE_LEFT),
            1.0,
            true,
        );
        self.push(
            [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
            shade(c, SHADE_RIGHT),
            1.0,
            true,
        );
        self.push(
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            shade(c, SHADE_TOP),
            1.0,
            true,
        );
        if self.swap {
            Frame {
                origin: [x0, y1, z1],
                u: [x1 - x0, 0.0, 0.0],
                v: [0.0, 0.0, z0 - z1],
                shade: SHADE_LEFT,
            }
        } else {
            Frame {
                origin: [x1, y1, z1],
                u: [0.0, y0 - y1, 0.0],
                v: [0.0, 0.0, z0 - z1],
                shade: SHADE_RIGHT,
            }
        }
    }

    /// A flat rectangle `(u0, v0)-(u1, v1)` of a face, shaded like the face.
    fn patch(&mut self, f: &Frame, (u0, v0, u1, v1): (f32, f32, f32, f32), c: Rgb) {
        self.push(
            [
                f.point(u0, v0),
                f.point(u1, v0),
                f.point(u1, v1),
                f.point(u0, v1),
            ],
            shade(c, f.shade),
            1.0,
            true,
        );
    }
}

const FACE_COLS: usize = 8;
const FACE_ROWS: usize = 6;
const FACE_BASE: [&str; FACE_ROWS] = [
    "........", ".ee..ee.", ".ee..ee.", "........", "..oooo..", "........",
];

/// The overlay of a face accessory: `f` frame, `s` sunglasses, `m` moustache, `k` mask.
fn face_overlay(face: Face) -> [&'static str; FACE_ROWS] {
    match face {
        Face::None => ["........"; FACE_ROWS],
        Face::Glasses => [
            "ffffffff", "f..ff..f", "f..ff..f", "ffffffff", "........", "........",
        ],
        Face::Sunglasses => [
            "........", "ssssssss", "ssssssss", "........", "........", "........",
        ],
        Face::Moustache => [
            "........", "........", "........", ".mmmmmm.", "m......m", "........",
        ],
        Face::Mask => [
            "........", "........", "........", "kkkkkkkk", "kkkkkkkk", "kkkkkkkk",
        ],
    }
}

fn character(look: &Look, swap: bool) -> Scene {
    let sk = skin(look);
    let mut s = Scene {
        swap,
        polys: Vec::new(),
    };
    // a light pad so that dark characters stand out and a shadow on it; flat, they do not bob
    for (half, c, alpha) in [(0.75, [1.0; 3], 0.14), (0.58, [0.0; 3], 0.24)] {
        s.push(
            [
                [-half, -half, 0.0],
                [half, -half, 0.0],
                [half, half, 0.0],
                [-half, half, 0.0],
            ],
            c,
            alpha,
            false,
        );
    }
    // the farther side (negative y) first
    s.cube((-0.25, 0.35), (-0.38, -0.05), (0.0, 0.25), sk.shoes);
    s.cube((-0.15, 0.15), (-0.72, -0.45), (0.45, 1.0), sk.arms);
    s.cube((-0.25, 0.35), (0.05, 0.38), (0.0, 0.25), sk.shoes);
    s.cube((-0.42, 0.42), (-0.42, 0.42), (0.25, 1.0), sk.body);
    s.cube((-0.44, 0.44), (-0.44, 0.44), (0.3, 0.45), sk.belt);
    s.cube((0.43, 0.5), (-0.1, 0.1), (0.31, 0.44), sk.buckle);
    s.cube((-0.15, 0.15), (0.45, 0.72), (0.45, 1.0), sk.arms);

    let female = look.gender == Gender::Female;
    let hair = mix(sk.helmet, [0.0; 3], 0.35);
    if female {
        s.cube((-0.15, 0.15), (-0.85, -0.62), (1.1, 1.6), hair);
    }
    let front = s.cube((-0.6, 0.6), (-0.6, 0.6), (1.0, 2.0), sk.helmet);
    if female {
        s.cube((-0.15, 0.15), (0.62, 0.85), (1.1, 1.6), hair);
    }

    // the pixel face on the front face of the head
    let (u0, v0, u1, v1) = (0.1, 0.12, 0.9, 0.92);
    s.patch(&front, (u0, v0, u1, v1), sk.face);
    let overlay = face_overlay(look.face);
    let cell = |row: usize, col: usize| -> Option<Rgb> {
        let o = overlay[row].as_bytes()[col];
        let ch = if o == b'.' {
            FACE_BASE[row].as_bytes()[col]
        } else {
            o
        };
        match ch {
            b'e' | b'o' => Some(sk.eyes),
            b'f' => Some(FRAME),
            b's' => Some(SUNGLASSES),
            b'm' => Some(MOUSTACHE),
            b'k' => Some(MASK),
            _ => None,
        }
    };
    let (du, dv) = ((u1 - u0) / FACE_COLS as f32, (v1 - v0) / FACE_ROWS as f32);
    for row in 0..FACE_ROWS {
        let mut col = 0;
        while col < FACE_COLS {
            let Some(c) = cell(row, col) else {
                col += 1;
                continue;
            };
            let mut end = col + 1;
            while end < FACE_COLS && cell(row, end) == Some(c) {
                end += 1;
            }
            s.patch(
                &front,
                (
                    u0 + du * col as f32,
                    v0 + dv * row as f32,
                    u0 + du * end as f32,
                    v0 + dv * (row + 1) as f32,
                ),
                c,
            );
            col = end;
        }
    }

    match look.head {
        Head::None => {}
        Head::Cap => {
            s.cube((-0.63, 0.63), (-0.63, 0.63), (2.0, 2.22), CAP);
            s.cube((0.6, 0.98), (-0.5, 0.5), (2.0, 2.08), CAP);
        }
        Head::TopHat => {
            s.cube((-0.8, 0.8), (-0.8, 0.8), (2.0, 2.08), HAT);
            s.cube((-0.45, 0.45), (-0.45, 0.45), (2.08, 2.75), HAT);
            s.cube((-0.47, 0.47), (-0.47, 0.47), (2.08, 2.25), HAT_BAND);
        }
        Head::Crown => {
            s.cube((-0.62, 0.62), (-0.62, 0.62), (2.0, 2.25), CROWN);
            for (x, y) in [(-0.62, -0.62), (0.42, -0.62), (-0.62, 0.42), (0.42, 0.42)] {
                s.cube((x, x + 0.2), (y, y + 0.2), (2.25, 2.5), CROWN);
            }
        }
        Head::Headphones => {
            s.cube((-0.1, 0.1), (-0.7, 0.7), (2.0, 2.12), HEADBAND);
            s.cube((-0.3, 0.3), (-0.82, -0.6), (1.25, 1.7), CUP);
            s.cube((-0.3, 0.3), (0.6, 0.82), (1.25, 1.7), CUP);
        }
    }
    s
}

/// The commands of one character fitted into `size` pixels, `phase` seconds into the animation.
///
/// Sizes that are not positive and finite give nothing. Every point lies inside the rectangle,
/// the commands are ordered back to front by their layer, the first two are the ground pad and shadow.
pub fn portrait3d(look: &Look, size: (f32, f32), phase: f32) -> Vec<Command> {
    let (w, h) = size;
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Vec::new();
    }
    let phase = if phase.is_finite() { phase } else { 0.0 };
    let scene = character(look, faces_left(phase));

    let screen: Vec<[(f32, f32); 4]> = scene.polys.iter().map(|p| p.points.map(project)).collect();
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for pt in screen.iter().flatten() {
        lo = (lo.0.min(pt.0), lo.1.min(pt.1));
        hi = (hi.0.max(pt.0), hi.1.max(pt.1));
    }
    let (bw, bh) = (
        (hi.0 - lo.0).max(f32::EPSILON),
        (hi.1 - lo.1).max(f32::EPSILON),
    );
    let scale = (w * 0.94 / bw).min(h * 0.88 / bh);
    let (cx, cy) = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
    let bob = bob_offset(phase, h);

    scene
        .polys
        .iter()
        .zip(&screen)
        .enumerate()
        .map(|(i, (poly, pts))| {
            let dy = if poly.bobs { bob } else { 0.0 };
            Command {
                kind: Kind::Quad,
                color: poly.color,
                points: pts.map(|p| {
                    (
                        w / 2.0 + (p.0 - cx) * scale,
                        h / 2.0 + (p.1 - cy) * scale + dy,
                    )
                }),
                size: 0.0,
                text: 0.0,
                layer: i as f32,
            }
        })
        .collect()
}
