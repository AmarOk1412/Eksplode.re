//! 2:1 isometric projection and fitting the arena into a canvas.

pub const TILE_W: f32 = 64.0;
pub const TILE_H: f32 = 32.0;
/// Height in pixels of a wall box.
pub const WALL_H: f32 = 32.0;
/// Empty border around the arena in the canvas.
pub const MARGIN: f32 = 16.0;

/// Projects tile coordinates `(x, y)` and a height `z` in pixels to view space.
pub fn project(x: f32, y: f32, z: f32) -> (f32, f32) {
    ((x - y) * TILE_W / 2.0, (x + y) * TILE_H / 2.0 - z)
}

/// Inverse of [`project`] on the ground plane (`z = 0`).
pub fn unproject(sx: f32, sy: f32) -> (f32, f32) {
    let a = sx / (TILE_W / 2.0);
    let b = sy / (TILE_H / 2.0);
    ((a + b) / 2.0, (b - a) / 2.0)
}

/// View-space bounding box `(min_x, min_y, max_x, max_y)` of a `width` x `height` arena with walls.
pub fn arena_bounds(width: u8, height: u8) -> (f32, f32, f32, f32) {
    let (w, h) = (f32::from(width), f32::from(height));
    let min_x = project(0.0, h, 0.0).0;
    let max_x = project(w, 0.0, 0.0).0;
    let min_y = project(0.0, 0.0, WALL_H).1;
    let max_y = project(w, h, 0.0).1;
    (min_x, min_y, max_x, max_y)
}

/// Scale and offset that map view space to canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl View {
    /// View-space point to canvas pixels.
    pub fn apply(&self, p: (f32, f32)) -> (f32, f32) {
        (
            p.0 * self.scale + self.offset_x,
            p.1 * self.scale + self.offset_y,
        )
    }

    /// Tile coordinates and height straight to canvas pixels.
    pub fn project(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        self.apply(project(x, y, z))
    }

    /// Canvas pixels to tile coordinates on the ground plane.
    pub fn unproject(&self, px: f32, py: f32) -> (f32, f32) {
        unproject(
            (px - self.offset_x) / self.scale,
            (py - self.offset_y) / self.scale,
        )
    }
}

/// Scales and centres the arena in a canvas, leaving [`MARGIN`] on every side.
pub fn fit(width: u8, height: u8, canvas: (f32, f32)) -> View {
    let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 1.0 };
    let (cw, ch) = (clean(canvas.0), clean(canvas.1));
    let (x0, y0, x1, y1) = arena_bounds(width, height);
    let avail_w = (cw - 2.0 * MARGIN).max(1.0);
    let avail_h = (ch - 2.0 * MARGIN).max(1.0);
    let scale = (avail_w / (x1 - x0)).min(avail_h / (y1 - y0));
    View {
        scale,
        offset_x: cw / 2.0 - scale * (x0 + x1) / 2.0,
        offset_y: ch / 2.0 - scale * (y0 + y1) / 2.0,
    }
}
