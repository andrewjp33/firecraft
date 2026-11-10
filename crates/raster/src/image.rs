//! Straight-alpha RGBA8 images.

use crate::mask::Mask;

/// One pixel, straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// What Fireworks MX leaves in a fully deleted pixel (observed: `00FFFFFF` ARGB).
    pub const CLEARED: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 0,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#rrggbb` or `#rrggbbaa`.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let v = u32::from_str_radix(s, 16).ok()?;
        match s.len() {
            6 => Some(Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)),
            8 => Some(Self::new(
                (v >> 24) as u8,
                (v >> 16) as u8,
                (v >> 8) as u8,
                v as u8,
            )),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Source-over composite of `src` onto `self` with an extra opacity (0..=255).
    pub fn over(self, src: Rgba, opacity: u8) -> Rgba {
        // round(alpha * opacity / 255), as Fireworks MX (an image at 50% object opacity).
        let v = src.a as u32 * opacity as u32;
        let sa = ((v + 128) + ((v + 128) >> 8)) >> 8;
        if sa == 0 {
            return self;
        }
        let da = self.a as u32;
        if da == 255 {
            // Onto an opaque pixel Fireworks MX rounds: round((s*a + d*(255-a)) / 255)
            // (measured with semi-transparent texture fills over white, 21,600 pixels exact).
            let mix = |s: u8, d: u8| {
                let v = s as u32 * sa + d as u32 * (255 - sa);
                (((v + 128) + ((v + 128) >> 8)) >> 8).min(255) as u8
            };
            return Rgba::new(mix(src.r, self.r), mix(src.g, self.g), mix(src.b, self.b), 255);
        }
        let out_a = sa + da * (255 - sa) / 255;
        if out_a == 0 {
            return Rgba::TRANSPARENT;
        }
        let mix = |s: u8, d: u8| -> u8 {
            ((s as u32 * sa + d as u32 * da * (255 - sa) / 255) / out_a).min(255) as u8
        };
        Rgba::new(
            mix(src.r, self.r),
            mix(src.g, self.g),
            mix(src.b, self.b),
            out_a as u8,
        )
    }
}

/// A rectangular RGBA image. Pixels are shared copy-on-write: cloning is cheap (undo snapshots
/// share unchanged bitmaps) and the first write to a shared image copies it.
#[derive(Clone)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: std::sync::Arc<Vec<Rgba>>,
    /// See [`stamp`](Self::stamp).
    stamp: u64,
}

impl PartialEq for RgbaImage {
    fn eq(&self, other: &Self) -> bool {
        (self.width, self.height) == (other.width, other.height) && self.pixels == other.pixels
    }
}

impl Eq for RgbaImage {}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RgbaImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels", &self.pixels)
            .finish()
    }
}

/// A stamp no image has had yet.
fn fresh_stamp() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Images larger than this many pixels are refused (64k x 64k guards hostile sizes).
pub const MAX_PIXELS: u64 = 1 << 32;

impl RgbaImage {
    /// A new image filled with `fill`. Sizes above [`MAX_PIXELS`] produce an empty image.
    pub fn new(width: u32, height: u32, fill: Rgba) -> Self {
        let n = width as u64 * height as u64;
        if n > MAX_PIXELS {
            return Self {
                width: 0,
                height: 0,
                pixels: Default::default(),
                stamp: fresh_stamp(),
            };
        }
        Self {
            width,
            height,
            pixels: std::sync::Arc::new(vec![fill; n as usize]),
            stamp: fresh_stamp(),
        }
    }

    /// Wrap pixels; `None` if the length does not match.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Rgba>) -> Option<Self> {
        (pixels.len() as u64 == width as u64 * height as u64).then(|| Self {
            width,
            height,
            pixels: std::sync::Arc::new(pixels),
            stamp: fresh_stamp(),
        })
    }

    /// From tightly packed RGBA bytes.
    pub fn from_rgba_bytes(width: u32, height: u32, bytes: &[u8]) -> Option<Self> {
        let px = bytes
            .chunks_exact(4)
            .map(|c| Rgba::new(c[0], c[1], c[2], c[3]))
            .collect();
        Self::from_pixels(width, height, px)
    }

    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect()
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[Rgba] {
        &self.pixels
    }
    pub fn pixels_mut(&mut self) -> &mut [Rgba] {
        self.writable().as_mut_slice()
    }

    /// Identifies these pixels: images with the same stamp have the same pixels (copies share
    /// it), and every write gives a new one. Unlike the buffer's address, a stamp is never
    /// reused, so caches can key on it safely (a freed buffer's address can come back for
    /// different pixels).
    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    /// The pixels, to change: copied first if shared, and stamped anew.
    fn writable(&mut self) -> &mut Vec<Rgba> {
        self.stamp = fresh_stamp();
        std::sync::Arc::make_mut(&mut self.pixels)
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64)
            .then(|| y as usize * self.width as usize + x as usize)
    }

    pub fn get(&self, x: i64, y: i64) -> Option<Rgba> {
        self.index(x, y).and_then(|i| self.pixels.get(i).copied())
    }

    pub fn set(&mut self, x: i64, y: i64, p: Rgba) {
        if let Some(i) = self.index(x, y) {
            if let Some(d) = self.writable().get_mut(i) {
                *d = p;
            }
        }
    }

    /// Remove the selected pixels (Edit > Clear / Delete). Partial coverage scales alpha;
    /// fully removed pixels become [`Rgba::CLEARED`] as in Fireworks MX.
    pub fn clear_masked(&mut self, mask: &Mask) {
        for (p, &m) in self.writable().iter_mut().zip(mask.data()) {
            if m == 0 {
                continue;
            }
            let a = p.a as u32 * (255 - m as u32) / 255;
            *p = if a == 0 {
                Rgba::CLEARED
            } else {
                Rgba { a: a as u8, ..*p }
            };
        }
    }

    /// The selected pixels as a new image (alpha scaled by coverage); unselected pixels are
    /// transparent. Used by Cut/Copy and "Cut to new layer".
    pub fn extract_masked(&self, mask: &Mask) -> RgbaImage {
        let pixels = self
            .pixels
            .iter()
            .zip(mask.data())
            .map(|(p, &m)| {
                let a = p.a as u32 * m as u32 / 255;
                if a == 0 {
                    Rgba::TRANSPARENT
                } else {
                    Rgba { a: a as u8, ..*p }
                }
            })
            .collect();
        RgbaImage {
            width: self.width,
            height: self.height,
            pixels: std::sync::Arc::new(pixels),
            stamp: fresh_stamp(),
        }
    }

    /// Composite `src` onto this image at offset (`dx`, `dy`).
    /// Composite `src` at (`dx`, `dy`) with a blend mode.
    pub fn draw_blend(
        &mut self,
        src: &RgbaImage,
        dx: i64,
        dy: i64,
        opacity: u8,
        mode: crate::blend::BlendMode,
    ) {
        self.draw_blend_with(src, dx, dy, opacity, mode, crate::ColorMath::Classic);
    }

    /// [`draw_blend`](Self::draw_blend) with a choice of colour-math precision.
    /// The part of `src` placed at (`dx`, `dy`) that lands on this image: (source x, source
    /// y, destination x, destination y, width, height).
    fn clip(&self, src: &RgbaImage, dx: i64, dy: i64) -> Option<(usize, usize, usize, usize, usize, usize)> {
        let x0 = dx.max(0);
        let y0 = dy.max(0);
        let x1 = (dx + src.width as i64).min(self.width as i64);
        let y1 = (dy + src.height as i64).min(self.height as i64);
        (x1 > x0 && y1 > y0).then(|| {
            (
                (x0 - dx) as usize,
                (y0 - dy) as usize,
                x0 as usize,
                y0 as usize,
                (x1 - x0) as usize,
                (y1 - y0) as usize,
            )
        })
    }

    /// Apply `f(destination, source)` to each overlapping pixel pair, row by row.
    fn blend_rows(&mut self, src: &RgbaImage, dx: i64, dy: i64, mut f: impl FnMut(&mut Rgba, Rgba)) {
        let Some((sx, sy, tx, ty, w, h)) = self.clip(src, dx, dy) else {
            return;
        };
        let (sw, tw) = (src.width as usize, self.width as usize);
        let dst = self.writable();
        for row in 0..h {
            let s0 = (sy + row) * sw + sx;
            let t0 = (ty + row) * tw + tx;
            let (Some(srow), Some(trow)) = (src.pixels.get(s0..s0 + w), dst.get_mut(t0..t0 + w)) else {
                continue;
            };
            for (d, &sp) in trow.iter_mut().zip(srow) {
                f(d, sp);
            }
        }
    }

    pub fn draw_blend_with(
        &mut self,
        src: &RgbaImage,
        dx: i64,
        dy: i64,
        opacity: u8,
        mode: crate::blend::BlendMode,
        math: crate::ColorMath,
    ) {
        if mode == crate::blend::BlendMode::Normal {
            return self.draw(src, dx, dy, opacity);
        }
        if opacity == 0 {
            return;
        }
        self.blend_rows(src, dx, dy, |d, s| {
            if s.a != 0 {
                *d = mode.composite_with(*d, s, opacity, math);
            }
        });
    }

    /// Replace the pixels under `src` (placed at `dx`, `dy`) with it.
    pub fn copy_from(&mut self, src: &RgbaImage, dx: i64, dy: i64) {
        self.blend_rows(src, dx, dy, |d, s| *d = s);
    }

    pub fn draw(&mut self, src: &RgbaImage, dx: i64, dy: i64, opacity: u8) {
        if opacity == 0 {
            return;
        }
        self.blend_rows(src, dx, dy, |d, s| {
            // `over`'s exact results for fully transparent and fully opaque sources.
            match (s.a, opacity) {
                (0, _) => {}
                (255, 255) => *d = s,
                _ => *d = d.over(s, opacity),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_is_correct_at_extremes() {
        let w = Rgba::WHITE;
        assert_eq!(w.over(Rgba::new(255, 0, 0, 255), 255), Rgba::new(255, 0, 0, 255));
        assert_eq!(w.over(Rgba::new(255, 0, 0, 0), 255), w);
        assert_eq!(
            Rgba::TRANSPARENT.over(Rgba::new(10, 20, 30, 128), 255),
            Rgba::new(10, 20, 30, 128)
        );
    }

    #[test]
    fn hex() {
        assert_eq!(Rgba::from_hex("#df0000"), Some(Rgba::new(0xdf, 0, 0, 255)));
        assert_eq!(Rgba::from_hex("#0000003f"), Some(Rgba::new(0, 0, 0, 0x3f)));
        assert_eq!(Rgba::from_hex("zz"), None);
    }

    #[test]
    fn clones_share_until_written() {
        let a = RgbaImage::new(4, 4, Rgba::WHITE);
        let mut b = a.clone();
        assert!(std::ptr::eq(a.pixels().as_ptr(), b.pixels().as_ptr()));
        b.set(0, 0, Rgba::BLACK);
        assert_eq!(a.get(0, 0), Some(Rgba::WHITE));
        assert_eq!(b.get(0, 0), Some(Rgba::BLACK));
    }

    /// Caches key images on their stamp: a buffer's address came back for different pixels once
    /// the old one was freed (gym3 on Windows: align, undo, then a stroke edit showed the
    /// undone alignment).
    #[test]
    fn stamps_follow_the_pixels_not_the_buffer() {
        let a = RgbaImage::new(4, 4, Rgba::WHITE);
        let mut b = a.clone();
        assert_eq!(a.stamp(), b.stamp(), "a copy shares the pixels");
        b.set(0, 0, Rgba::BLACK);
        assert_ne!(a.stamp(), b.stamp(), "a write");
        // Written in place (the buffer not shared, so not moved): still a new stamp.
        let before = b.stamp();
        b.pixels_mut()[1] = Rgba::BLACK;
        assert_ne!(before, b.stamp());
        // Equal pixels made separately are equal images, with their own stamps.
        let c = RgbaImage::new(4, 4, Rgba::WHITE);
        assert_eq!(a, c);
        assert_ne!(a.stamp(), c.stamp());
        // A buffer freed and allocated again for other pixels: a new stamp whatever its address.
        let first = RgbaImage::new(64, 64, Rgba::WHITE);
        let old = first.stamp();
        drop(first);
        let second = RgbaImage::new(64, 64, Rgba::BLACK);
        assert_ne!(old, second.stamp());
    }

    #[test]
    fn huge_sizes_do_not_allocate() {
        assert_eq!(RgbaImage::new(u32::MAX, u32::MAX, Rgba::WHITE).width(), 0);
    }
}
