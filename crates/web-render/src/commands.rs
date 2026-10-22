//! Turns the draw list into a flat buffer of drawing commands for the page.

use bomberust_core::{Observation, PowerUpKind};

use crate::cube::{
    badge_char, box_faces, player_color, shade, Faces, BLOCK_LEFT, BLOCK_RIGHT, BLOCK_TOP,
    FACE_LEFT, FACE_RIGHT, FACE_TOP, FLOOR_A, FLOOR_B, WALL_LEFT, WALL_RIGHT, WALL_TOP,
};
use crate::depth::{draw_list, Item, Sprite};
use crate::interp::Interp;
use crate::iso::{fit, View, WALL_H};

/// What a command draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Quad = 0,
    Circle = 1,
    Text = 2,
    Triangle = 3,
}

/// One drawing command; colours are 0.0-1.0, points are canvas pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub kind: Kind,
    pub color: [f32; 4],
    pub points: [(f32, f32); 4],
    pub size: f32,
    pub text: f32,
    pub layer: f32,
}

impl Command {
    /// Floats per command in the flat buffer.
    pub const STRIDE: usize = 16;

    pub fn to_array(&self) -> [f32; 16] {
        let p = &self.points;
        [
            f32::from(self.kind as u8),
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
            p[0].0,
            p[0].1,
            p[1].0,
            p[1].1,
            p[2].0,
            p[2].1,
            p[3].0,
            p[3].1,
            self.size,
            self.text,
            self.layer,
        ]
    }
}

/// The commands as one flat `f32` buffer, `Command::STRIDE` floats each.
pub fn flatten(list: &[Command]) -> Vec<f32> {
    list.iter().flat_map(Command::to_array).collect()
}

fn opaque(c: [f32; 3]) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

fn quad(color: [f32; 4], p: [(f32, f32); 4], layer: f32) -> Command {
    Command {
        kind: Kind::Quad,
        color,
        points: p,
        size: 0.0,
        text: 0.0,
        layer,
    }
}

fn text(color: [f32; 4], at: (f32, f32), size: f32, ch: char, layer: f32) -> Command {
    let mut points = [(0.0, 0.0); 4];
    points[0] = at;
    Command {
        kind: Kind::Text,
        color,
        points,
        size,
        text: f32::from(ch as u8),
        layer,
    }
}

/// Builds the commands for one frame. `alpha` (0.0-1.0) is the progress between two ticks.
pub fn build(obs: &Observation, interp: &Interp, alpha: f32, canvas: (f32, f32)) -> Vec<Command> {
    let view = fit(obs.arena.width(), obs.arena.height(), canvas);
    let phase = obs.tick as f32 + if alpha.is_finite() { alpha } else { 0.0 };
    let mut out = Vec::new();
    for it in draw_list(obs, interp, alpha, phase) {
        emit(&mut out, &view, &it);
    }
    out
}

fn map_quad(view: &View, p: [(f32, f32); 4]) -> [(f32, f32); 4] {
    [
        view.apply(p[0]),
        view.apply(p[1]),
        view.apply(p[2]),
        view.apply(p[3]),
    ]
}

fn diamond(view: &View, c: (f32, f32), footprint: f32, z: f32) -> [(f32, f32); 4] {
    map_quad(view, box_faces(c.0, c.1, footprint, z, 0.0).top)
}

fn faces(out: &mut Vec<Command>, view: &View, f: Faces, colors: [[f32; 3]; 3], layer: f32) {
    out.push(quad(opaque(colors[0]), map_quad(view, f.left), layer));
    out.push(quad(opaque(colors[1]), map_quad(view, f.right), layer));
    out.push(quad(opaque(colors[2]), map_quad(view, f.top), layer));
}

fn emit(out: &mut Vec<Command>, view: &View, it: &Item) {
    let (cx, cy) = it.pos;
    let layer = it.key();
    match it.sprite {
        Sprite::Floor { dark } => {
            let c = if dark { FLOOR_B } else { FLOOR_A };
            out.push(quad(opaque(c), diamond(view, it.pos, 1.0, 0.0), -1.0));
        }
        Sprite::Wall => {
            let f = box_faces(cx, cy, 1.0, 0.0, WALL_H);
            faces(out, view, f, [WALL_LEFT, WALL_RIGHT, WALL_TOP], layer);
        }
        Sprite::Block => {
            let f = box_faces(cx, cy, 0.94, 0.0, 18.0);
            faces(out, view, f, [BLOCK_LEFT, BLOCK_RIGHT, BLOCK_TOP], layer);
        }
        Sprite::Flame => {
            out.push(quad(
                [1.0, 0.45, 0.1, 0.75],
                diamond(view, it.pos, 1.0, 1.0),
                layer,
            ));
            out.push(quad(
                [1.0, 0.9, 0.3, 0.85],
                diamond(view, it.pos, 0.5, 2.0),
                layer,
            ));
        }
        Sprite::PowerUp(kind) => {
            let lift = it.lift;
            out.push(quad(
                [0.2, 0.8, 0.9, 1.0],
                diamond(view, it.pos, 0.4, 4.0 + lift),
                layer,
            ));
            let letter = match kind {
                PowerUpKind::ExtraBomb => 'B',
                PowerUpKind::Range => 'R',
                PowerUpKind::Speed => 'S',
            };
            let at = view.project(cx, cy, 22.0 + lift);
            out.push(text(
                [1.0, 1.0, 1.0, 1.0],
                at,
                14.0 * view.scale,
                letter,
                layer,
            ));
        }
        Sprite::Bomb { ticks_left } => {
            let pulse = 0.5 * (1.0 + (ticks_left as f32 * 0.8).sin());
            let radius = 10.0 * view.scale * (1.0 + 0.2 * pulse);
            let mut points = [(0.0, 0.0); 4];
            points[0] = view.project(cx, cy, 12.0);
            out.push(Command {
                kind: Kind::Circle,
                color: [0.08, 0.08, 0.1, 1.0],
                points,
                size: radius,
                text: 0.0,
                layer,
            });
        }
        Sprite::Player { slot } => {
            let c = player_color(slot);
            let f = box_faces(cx, cy, 0.6, 0.0, 26.0);
            let colors = [
                shade(c, FACE_LEFT),
                shade(c, FACE_RIGHT),
                shade(c, FACE_TOP),
            ];
            faces(out, view, f, colors, layer);
            let top = view.apply(f.top[0]);
            let bottom = view.apply(f.top[2]);
            let mid = ((top.0 + bottom.0) / 2.0, (top.1 + bottom.1) / 2.0);
            out.push(text(
                [1.0, 1.0, 1.0, 1.0],
                mid,
                16.0 * view.scale,
                badge_char(slot),
                layer,
            ));
        }
    }
}
