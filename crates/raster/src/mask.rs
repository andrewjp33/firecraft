//! 8-bit selection masks (0 = unselected, 255 = fully selected).

/// How a new selection combines with an existing one (Fireworks: replace/add/subtract/intersect).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Combine {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Mask {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as u64 * height as u64).min(crate::image::MAX_PIXELS) as usize;
        Self {
            width,
            height,
            data: vec![0; n],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64)
            .then(|| y as usize * self.width as usize + x as usize)
    }

    pub fn get(&self, x: i64, y: i64) -> u8 {
        self.index(x, y)
            .and_then(|i| self.data.get(i).copied())
            .unwrap_or(0)
    }

    pub fn set(&mut self, x: i64, y: i64, v: u8) {
        if let Some(i) = self.index(x, y) {
            if let Some(d) = self.data.get_mut(i) {
                *d = v;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|&v| v == 0)
    }

    /// Bounding box of selected pixels as (x, y, w, h).
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x as i64, y as i64) > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    /// Combine `other` into this mask.
    pub fn combine(&mut self, other: &Mask, mode: Combine) {
        if other.width != self.width || other.height != self.height {
            return;
        }
        for (d, &s) in self.data.iter_mut().zip(&other.data) {
            *d = match mode {
                Combine::Replace => s,
                Combine::Add => (*d).max(s),
                Combine::Subtract => ((*d as u32) * (255 - s as u32) / 255) as u8,
                Combine::Intersect => ((*d as u32) * (s as u32) / 255) as u8,
            };
        }
    }

    pub fn invert(&mut self) {
        self.data.iter_mut().for_each(|v| *v = 255 - *v);
    }

    /// Feather: soften the edge by `radius` pixels (three box-blur passes approximate a
    /// Gaussian). Fireworks clamps feather amounts to 0..=32000; we cap the radius at 250.
    pub fn feather(&mut self, radius: u32) {
        let r = radius.min(250) as i64;
        if r == 0 {
            return;
        }
        let r = (r / 2).max(1);
        for _ in 0..3 {
            self.box_blur(r, true);
            self.box_blur(r, false);
        }
    }

    fn box_blur(&mut self, r: i64, horizontal: bool) {
        let (w, h) = (self.width as i64, self.height as i64);
        let src = self.data.clone();
        let at = |x: i64, y: i64| -> u32 {
            if x < 0 || y < 0 || x >= w || y >= h {
                return 0;
            }
            src.get((y * w + x) as usize).copied().unwrap_or(0) as u32
        };
        let n = (2 * r + 1) as u32;
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let mut sum: u32 = 0;
            for k in -r..=r {
                sum += if horizontal { at(k, o) } else { at(o, k) };
            }
            for i in 0..inner {
                let (x, y) = if horizontal { (i, o) } else { (o, i) };
                if let Some(d) = self.data.get_mut((y * w + x) as usize) {
                    *d = (sum / n) as u8;
                }
                let (add, sub) = if horizontal {
                    (at(i + r + 1, o), at(i - r, o))
                } else {
                    (at(o, i + r + 1), at(o, i - r))
                };
                sum = sum + add - sub;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_modes() {
        let mut a = Mask::new(2, 1);
        a.set(0, 0, 255);
        let mut b = Mask::new(2, 1);
        b.set(1, 0, 255);
        let mut add = a.clone();
        add.combine(&b, Combine::Add);
        assert_eq!(add.data(), &[255, 255]);
        let mut sub = add.clone();
        sub.combine(&b, Combine::Subtract);
        assert_eq!(sub.data(), &[255, 0]);
        let mut int = add.clone();
        int.combine(&a, Combine::Intersect);
        assert_eq!(int.data(), &[255, 0]);
    }

    #[test]
    fn feather_softens_but_keeps_mass_centered() {
        let mut m = Mask::new(21, 21);
        for y in 5..16 {
            for x in 5..16 {
                m.set(x, y, 255);
            }
        }
        m.feather(4);
        assert!(m.get(10, 10) > 200);
        assert!(m.get(5, 10) > 60 && m.get(5, 10) < 200);
        assert_eq!(m.get(0, 0), 0);
        assert_eq!(m.bounds().map(|b| b.0 < 5), Some(true));
    }
}
