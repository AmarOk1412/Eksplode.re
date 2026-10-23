//! Turns the draw list into a flat buffer of drawing commands for the page.

use bomberust_core::{Observation, PowerUpKind};

use crate::depth::{draw_list, Item, Sprite};
use crate::interp::Interp;
use crate::look::{default_for, runs_for, runs_of, Look, GRID_ROWS, OVERHEAD};
use crate::sprites::{
    bomb_colour, hex, runs, Rgb, Run, BOARD, BOARD_EDGE, BOMB, BRICK_BASE, BRICK_LINE, FLAME_CORE,
    FLAME_MID, FLAME_OUTER, PLAYER, WALL_BASE, WALL_SHADE, WALL_WINDOW, WALL_WINDOW_EDGE, WHITE,
};
use crate::view::{fit, View, EDGE};

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

fn solid(c: Rgb) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

/// Draws rectangles in tile coordinates of one frame.
struct Pen<'a> {
    view: &'a View,
    out: &'a mut Vec<Command>,
    layer: f32,
}

impl Pen<'_> {
    fn rect(&mut self, c: Rgb, x0: f32, y0: f32, x1: f32, y1: f32) {
        let p = self.view.rect(x0, y0, x1, y1);
        self.out.push(quad(solid(c), p, self.layer));
    }

    fn shadow(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let p = self.view.rect(x0, y0, x1, y1);
        self.out.push(quad([0.0, 0.0, 0.0, 0.14], p, self.layer));
    }

    /// Pixels of a sprite: `ps` canvas pixels per sprite pixel, top-left at `(x, y)` in pixels.
    fn sprite(&mut self, list: &[Run], ps: f32, x: f32, y: f32) {
        for r in list {
            let (x0, y0) = (x + r.col as f32 * ps, y + r.row as f32 * ps);
            let p = [
                (x0, y0),
                (x0 + r.len as f32 * ps, y0),
                (x0 + r.len as f32 * ps, y0 + ps),
                (x0, y0 + ps),
            ];
            self.out.push(quad(solid(r.color), p, self.layer));
        }
    }
}

/// Builds the commands for one frame. `alpha` (0.0-1.0) is the progress between two ticks.
pub fn build(obs: &Observation, interp: &Interp, alpha: f32, canvas: (f32, f32)) -> Vec<Command> {
    build_with_looks(obs, interp, alpha, canvas, &[0, 1, 2, 3].map(default_for))
}

/// Like [`build`] with the look of each slot.
pub fn build_with_looks(
    obs: &Observation,
    interp: &Interp,
    alpha: f32,
    canvas: (f32, f32),
    looks: &[Look; 4],
) -> Vec<Command> {
    let (w, h) = (obs.arena.width(), obs.arena.height());
    let view = fit(w, h, canvas);
    let phase = obs.tick as f32 + if alpha.is_finite() { alpha } else { 0.0 };
    let mut out = Vec::new();
    let (wf, hf) = (f32::from(w), f32::from(h));
    {
        let mut pen = Pen {
            view: &view,
            out: &mut out,
            layer: -1.0,
        };
        pen.rect(BOARD_EDGE, 0.0, hf, wf, hf + EDGE);
        pen.rect(BOARD, 0.0, 0.0, wf, hf);
    }
    for it in draw_list(obs, interp, alpha, phase) {
        emit(&mut out, &view, &it, looks);
    }
    out
}

fn emit(out: &mut Vec<Command>, view: &View, it: &Item, looks: &[Look; 4]) {
    let (cx, cy) = it.pos;
    let (tx, ty) = (cx - 0.5, cy - 0.5);
    let mut pen = Pen {
        view,
        out,
        layer: it.key(),
    };
    match it.sprite {
        Sprite::Wall => {
            pen.rect(WALL_BASE, tx, ty, tx + 1.0, ty + 1.0);
            pen.rect(WALL_SHADE, tx, ty + 0.88, tx + 1.0, ty + 1.0);
            pen.rect(WALL_WINDOW_EDGE, tx + 0.30, ty + 0.56, tx + 0.70, ty + 0.60);
            pen.rect(WALL_WINDOW, tx + 0.30, ty + 0.60, tx + 0.70, ty + 0.78);
        }
        Sprite::Block => {
            pen.rect(BRICK_BASE, tx, ty, tx + 1.0, ty + 1.0);
            let courses = [0.0, 0.33, 0.66, 0.99];
            for b in courses {
                pen.rect(
                    BRICK_LINE,
                    tx,
                    ty + (b - 0.035_f32).max(0.0),
                    tx + 1.0,
                    ty + (b + 0.035_f32).min(1.0),
                );
            }
            for (k, pair) in courses.windows(2).enumerate() {
                let joints: &[f32] = if k % 2 == 0 { &[0.5] } else { &[0.22, 0.78] };
                for x in joints {
                    pen.rect(
                        BRICK_LINE,
                        tx + x - 0.035,
                        ty + pair[0],
                        tx + x + 0.035,
                        ty + pair[1],
                    );
                }
            }
        }
        Sprite::Flame => {
            pen.rect(FLAME_OUTER, tx, ty, tx + 1.0, ty + 1.0);
            pen.rect(FLAME_MID, tx + 0.18, ty + 0.18, tx + 0.82, ty + 0.82);
            pen.rect(FLAME_CORE, tx + 0.36, ty + 0.36, tx + 0.64, ty + 0.64);
        }
        Sprite::PowerUp(kind) => {
            let (color, letter) = match kind {
                PowerUpKind::ExtraBomb => (hex(0x3b4cc0), 'B'),
                PowerUpKind::Range => (hex(0xe8661a), 'R'),
                PowerUpKind::Speed => (hex(0x2fa84f), 'S'),
            };
            let dy = it.lift;
            pen.rect(WHITE, tx + 0.2, ty + 0.2 + dy, tx + 0.8, ty + 0.8 + dy);
            pen.rect(color, tx + 0.26, ty + 0.26 + dy, tx + 0.74, ty + 0.74 + dy);
            let at = view.project(cx, cy + dy);
            pen.out
                .push(text([1.0; 4], at, 0.4 * view.scale, letter, it.key()));
        }
        Sprite::Bomb { ticks_left } => {
            let pulse = 0.5 * (1.0 + (ticks_left as f32 * 0.8).sin());
            let ps = (0.8 * view.scale / 10.0 * (1.0 + 0.15 * pulse))
                .round()
                .max(1.0);
            let blink = ticks_left % 4 < 2;
            let list = runs(&BOMB, BOMB.len(), |c| bomb_colour(c, blink));
            let (px, py) = view.project(cx, cy);
            pen.sprite(
                &list,
                ps,
                (px - 5.0 * ps).round(),
                (py - 5.0 * ps + 0.05 * view.scale).round(),
            );
        }
        Sprite::Player { slot } => {
            let list = runs_for(&looks[usize::from(slot) % 4]);
            let ps = (1.2 * view.scale / PLAYER.len() as f32).round().max(1.0);
            let (px, py) = view.project(cx, cy);
            pen.shadow(cx - 0.3, cy + 0.34, cx + 0.3, cy + 0.44);
            let top = py + 0.5 * view.scale - GRID_ROWS as f32 * ps - 0.02 * view.scale;
            pen.sprite(&list, ps, (px - 6.0 * ps).round(), top.round());
        }
    }
}

/// The default character of `slot` fitted into a canvas; see [`portrait_look`].
pub fn portrait(slot: u8, size: (f32, f32), rows: usize) -> Vec<Command> {
    portrait_look(&default_for(usize::from(slot)), size, rows)
}

/// A character fitted into a canvas of `size` pixels: the whole figure for `rows == 0`,
/// otherwise the head and shoulders of its first `rows` character rows, zoomed and cropped at the
/// sides.
pub fn portrait_look(look: &Look, size: (f32, f32), rows: usize) -> Vec<Command> {
    let (w, h) = size;
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Vec::new();
    }
    let total = if rows == 0 {
        GRID_ROWS
    } else {
        (rows + OVERHEAD).min(GRID_ROWS)
    };
    let list = runs_of(look, total);
    // a crop is zoomed so the head fills the card and the arms run off its sides
    let wide = if total < GRID_ROWS { 1.5 } else { 0.9 };
    let ps = (wide * w / 12.0)
        .min(0.9 * h / total as f32)
        .floor()
        .max(1.0);
    let x = ((w - 12.0 * ps) / 2.0).round();
    let y = ((h - total as f32 * ps) / 2.0).round();
    let mut out = Vec::new();
    let view = View {
        scale: 1.0,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    Pen {
        view: &view,
        out: &mut out,
        layer: 0.0,
    }
    .sprite(&list, ps, x, y);
    out
}
