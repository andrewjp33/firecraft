//! Pattern and Web Dither fills. Pixel mapping as measured by FirePixel on the original
//! (`fp-vector/src/paint.rs`, Apache-2.0; see ATTRIBUTION.md).

use crate::image::{Rgba, RgbaImage};

/// A bitmap repeated across a fill, placed by three handles.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternFill {
    pub name: String,
    pub image: RgbaImage,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub side: (f64, f64),
}

impl PatternFill {
    /// The pattern's colour at a canvas point: its pixel (0, 0) sits on the round handle, its
    /// columns run towards the end handle and its rows away from the side handle.
    pub fn color_at(&self, x: f64, y: f64) -> Rgba {
        let (w, h) = (self.image.width() as f64, self.image.height() as f64);
        let (ax, ay) = (self.end.0 - self.start.0, self.end.1 - self.start.1);
        let (bx, by) = (self.side.0 - self.start.0, self.side.1 - self.start.1);
        let det = ax * by - ay * bx;
        let (qx, qy) = (x - self.start.0, y - self.start.1);
        let (u, v) = if det.abs() < 1e-12 {
            (0.0, 0.0)
        } else {
            ((qx * by - qy * bx) / det, (ax * qy - ay * qx) / det)
        };
        // A tiny tolerance keeps values such as 36.999999999 on the integer they stand for.
        let wrap = |t: f64, size: f64| ((t + 1e-7).floor().rem_euclid(size.max(1.0))) as i64;
        self.image
            .get(wrap(u * w, w), wrap(-v * h, h))
            .unwrap_or(Rgba::TRANSPARENT)
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for p in [&mut self.start, &mut self.end, &mut self.side] {
            p.0 += dx;
            p.1 += dy;
        }
    }
}

/// A Web Dither fill's colour at a canvas pixel: a 2 x 2 tile of `colors`, by rows.
pub fn web_dither_at(colors: &[Rgba; 4], x: f64, y: f64) -> Rgba {
    let bit = |v: f64| (v.round() as i64).rem_euclid(2) as usize;
    colors[bit(y) * 2 + bit(x)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_origin_sits_on_the_round_handle() {
        let mut image = RgbaImage::new(4, 4, Rgba::WHITE);
        image.set(0, 0, Rgba::BLACK);
        image.set(1, 0, Rgba::new(255, 0, 0, 255));
        let p = PatternFill {
            name: "t".into(),
            image,
            start: (10.0, 20.0),
            end: (14.0, 20.0),
            side: (10.0, 16.0),
        };
        assert_eq!(p.color_at(10.0, 20.0), Rgba::BLACK);
        assert_eq!(p.color_at(11.0, 20.0), Rgba::new(255, 0, 0, 255));
        // It repeats with no seam before the handle.
        assert_eq!(p.color_at(6.0, 20.0), Rgba::BLACK);
        assert_eq!(p.color_at(10.0, 24.0), Rgba::BLACK);
    }

    #[test]
    fn web_dither_is_a_two_by_two_tile() {
        let (a, b) = (Rgba::BLACK, Rgba::WHITE);
        let c = [a, b, b, a];
        assert_eq!(web_dither_at(&c, 0.0, 0.0), a);
        assert_eq!(web_dither_at(&c, 1.0, 0.0), b);
        assert_eq!(web_dither_at(&c, 0.0, 1.0), b);
        assert_eq!(web_dither_at(&c, 1.0, 1.0), a);
    }
}
