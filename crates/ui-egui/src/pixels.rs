//! Drawing whole physical screen pixels, whatever the zoom and the display scaling.
//!
//! egui paints in points; with display scaling (Windows 125%, HiDPI) a point isn't a pixel, so
//! 1-point dashes land between pixels and come out uneven. Selection outlines (the marching
//! ants, the crop box's dashes) are drawn here instead, on the physical pixel grid, as
//! Fireworks MX draws them: their dashes are screen pixels long at any zoom.
//!
//! Pixels go into one mesh as plain squares, a run of same-coloured pixels as one rectangle
//! (1 px line segments would have their ends rounded by the tessellator).

use egui::{Color32, Mesh, Pos2, Rect, pos2};

pub(crate) struct PixelMesh {
    mesh: Mesh,
    ppp: f32,
    /// The painter's clip rectangle in physical pixels: nothing outside is drawn.
    clip: (i64, i64, i64, i64),
}

impl PixelMesh {
    pub(crate) fn new(p: &egui::Painter) -> Self {
        let ppp = p.ctx().pixels_per_point();
        let c = p.clip_rect();
        let px = |v: f32| (v * ppp).floor() as i64;
        Self {
            mesh: Mesh::default(),
            ppp,
            clip: (px(c.min.x) - 1, px(c.min.y) - 1, px(c.max.x) + 1, px(c.max.y) + 1),
        }
    }

    /// Point `q` on the physical pixel grid: the pixel boundary nearest it.
    pub(crate) fn grid(&self, q: Pos2) -> (i64, i64) {
        ((q.x * self.ppp).round() as i64, (q.y * self.ppp).round() as i64)
    }

    /// `points` interface points in whole physical pixels (at least 1).
    pub(crate) fn pixels(&self, points: f32) -> i64 {
        ((points * self.ppp).round() as i64).max(1)
    }

    /// The pixels `x0..x1` by `y0..y1`, filled.
    pub(crate) fn rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: Color32) {
        let s = 1.0 / self.ppp;
        let r = Rect::from_min_max(pos2(x0 as f32 * s, y0 as f32 * s), pos2(x1 as f32 * s, y1 as f32 * s));
        self.mesh.add_colored_rect(r, c);
    }

    /// Pixels `x0..x1` of row `y`, each coloured by `color(x, y)`.
    pub(crate) fn row(&mut self, y: i64, x0: i64, x1: i64, color: impl Fn(i64, i64) -> Color32) {
        if y < self.clip.1 || y >= self.clip.3 {
            return;
        }
        let (x0, x1) = (x0.max(self.clip.0), x1.min(self.clip.2));
        let mut start = x0;
        while start < x1 {
            let c = color(start, y);
            let mut end = start + 1;
            while end < x1 && color(end, y) == c {
                end += 1;
            }
            self.rect(start, y, end, y + 1, c);
            start = end;
        }
    }

    /// Pixels `y0..y1` of column `x`, each coloured by `color(x, y)`.
    pub(crate) fn column(&mut self, x: i64, y0: i64, y1: i64, color: impl Fn(i64, i64) -> Color32) {
        if x < self.clip.0 || x >= self.clip.2 {
            return;
        }
        let (y0, y1) = (y0.max(self.clip.1), y1.min(self.clip.3));
        let mut start = y0;
        while start < y1 {
            let c = color(x, start);
            let mut end = start + 1;
            while end < y1 && color(x, end) == c {
                end += 1;
            }
            self.rect(x, start, x + 1, end, c);
            start = end;
        }
    }

    /// The outline of pixels `l..=r` by `t..=b`: its outermost pixels.
    pub(crate) fn outline(&mut self, (l, t, r, b): (i64, i64, i64, i64), color: impl Fn(i64, i64) -> Color32) {
        let (r, b) = (r.max(l), b.max(t));
        self.row(t, l, r + 1, &color);
        if b > t {
            self.row(b, l, r + 1, &color);
        }
        self.column(l, t + 1, b, &color);
        if r > l {
            self.column(r, t + 1, b, &color);
        }
    }

    pub(crate) fn paint(self, p: &egui::Painter) {
        p.add(egui::Shape::mesh(self.mesh));
    }
}

/// Marching ants, as Fireworks MX draws them: 4 screen pixels black, 4 white, in diagonal
/// stripes across the screen (the colour depends only on x - y, measured 2026-11-26), moving
/// with `phase`.
pub(crate) fn ant(phase: i64) -> impl Fn(i64, i64) -> Color32 {
    move |x, y| {
        if (x - y + phase).rem_euclid(8) < 4 {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ants_are_diagonal_stripes_of_four() {
        // Fireworks MX's top row of a marquee at y 457: black from x 443 to 446.
        let a = ant(-2);
        let row: String = (442..452).map(|x| if a(x, 457) == Color32::BLACK { 'K' } else { 'W' }).collect();
        assert_eq!(row, "WKKKKWWWWK");
        // Down a column the stripes run the other way.
        assert_eq!(a(501, 457), Color32::BLACK);
        assert_eq!(a(501, 460), Color32::WHITE);
    }
}
