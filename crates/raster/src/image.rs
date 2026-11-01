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
        let sa = src.a as u32 * opacity as u32 / 255;
        if sa == 0 {
            return self;
        }
        let da = self.a as u32;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: std::sync::Arc<Vec<Rgba>>,
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
            };
        }
        Self {
            width,
            height,
            pixels: std::sync::Arc::new(vec![fill; n as usize]),
        }
    }

    /// Wrap pixels; `None` if the length does not match.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Rgba>) -> Option<Self> {
        (pixels.len() as u64 == width as u64 * height as u64).then(|| Self {
            width,
            height,
            pixels: std::sync::Arc::new(pixels),
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
        std::sync::Arc::make_mut(&mut self.pixels).as_mut_slice()
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
            if let Some(d) = std::sync::Arc::make_mut(&mut self.pixels).get_mut(i) {
                *d = p;
            }
        }
    }

    /// Remove the selected pixels (Edit > Clear / Delete). Partial coverage scales alpha;
    /// fully removed pixels become [`Rgba::CLEARED`] as in Fireworks MX.
    pub fn clear_masked(&mut self, mask: &Mask) {
        for (p, &m) in std::sync::Arc::make_mut(&mut self.pixels)
            .iter_mut()
            .zip(mask.data())
        {
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
        for y in 0..src.height as i64 {
            for x in 0..src.width as i64 {
                let (Some(s), Some(i)) = (src.get(x, y), self.index(x + dx, y + dy)) else {
                    continue;
                };
                if s.a == 0 {
                    continue;
                }
                if let Some(d) = std::sync::Arc::make_mut(&mut self.pixels).get_mut(i) {
                    *d = mode.composite_with(*d, s, opacity, math);
                }
            }
        }
    }

    pub fn draw(&mut self, src: &RgbaImage, dx: i64, dy: i64, opacity: u8) {
        for y in 0..src.height as i64 {
            for x in 0..src.width as i64 {
                let (Some(s), Some(i)) = (src.get(x, y), self.index(x + dx, y + dy)) else {
                    continue;
                };
                if let Some(d) = std::sync::Arc::make_mut(&mut self.pixels).get_mut(i) {
                    *d = d.over(s, opacity);
                }
            }
        }
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

    #[test]
    fn huge_sizes_do_not_allocate() {
        assert_eq!(RgbaImage::new(u32::MAX, u32::MAX, Rgba::WHITE).width(), 0);
    }
}
