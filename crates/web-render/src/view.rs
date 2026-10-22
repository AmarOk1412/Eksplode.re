//! Top-down view: fitting the arena into a canvas and snapping rectangles to whole pixels.

/// Empty border around the arena in the canvas.
pub const MARGIN: f32 = 16.0;
/// Thickness of the board's front edge, in tiles.
pub const EDGE: f32 = 0.18;

/// View-space bounding box `(min_x, min_y, max_x, max_y)` of a `width` x `height` arena including its edge.
pub fn arena_bounds(width: u8, height: u8) -> (f32, f32, f32, f32) {
    (0.0, 0.0, f32::from(width), f32::from(height) + EDGE)
}

/// Scale and offset that map tile coordinates to canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl View {
    /// Tile coordinates to canvas pixels.
    pub fn project(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x * self.scale + self.offset_x,
            y * self.scale + self.offset_y,
        )
    }

    /// Canvas pixels to tile coordinates.
    pub fn unproject(&self, px: f32, py: f32) -> (f32, f32) {
        (
            (px - self.offset_x) / self.scale,
            (py - self.offset_y) / self.scale,
        )
    }

    /// The rectangle between two tile-coordinate corners as four canvas points, snapped to whole
    /// pixels so neighbouring tiles share their edges exactly. It is never thinner than one pixel.
    pub fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> [(f32, f32); 4] {
        let (ax, ay) = self.project(x0, y0);
        let (bx, by) = self.project(x1, y1);
        let (ax, ay) = (ax.round(), ay.round());
        let bx = bx.round().max(ax + 1.0);
        let by = by.round().max(ay + 1.0);
        [(ax, ay), (bx, ay), (bx, by), (ax, by)]
    }
}

/// Scales and centres the arena in a canvas, leaving [`MARGIN`] on every side.
pub fn fit(width: u8, height: u8, canvas: (f32, f32)) -> View {
    let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 1.0 };
    let (cw, ch) = (clean(canvas.0), clean(canvas.1));
    let (x0, y0, x1, y1) = arena_bounds(width, height);
    let avail_w = (cw - 2.0 * MARGIN).max(1.0);
    let avail_h = (ch - 2.0 * MARGIN).max(1.0);
    let scale = (avail_w / (x1 - x0))
        .min(avail_h / (y1 - y0))
        .floor()
        .max(1.0);
    View {
        scale,
        offset_x: (cw / 2.0 - scale * (x0 + x1) / 2.0).round(),
        offset_y: (ch / 2.0 - scale * (y0 + y1) / 2.0).round(),
    }
}
