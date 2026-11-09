//! The geometry, colour and image types the text engine uses (reduced from FirePixel's fp-geom,
//! fp-color and fp-raster).

pub use firecraft_stroke::support::geom::{BezierContour, BezierNode, IRect, Point};

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}

/// An 8-bit colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// An 8-bit coverage mask over a pixel rectangle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mask {
    bounds: IRect,
    data: Vec<u8>,
}

impl Mask {
    pub fn empty_at(x: i32, y: i32) -> Self {
        Self {
            bounds: IRect::new(x, y, x, y),
            data: Vec::new(),
        }
    }
    pub fn from_raw(bounds: IRect, data: Vec<u8>) -> Result<Self, ()> {
        let (w, h) = (bounds.width().max(0) as usize, bounds.height().max(0) as usize);
        if data.len() == w * h {
            Ok(Self { bounds, data })
        } else {
            Err(())
        }
    }
    pub fn bounds(&self) -> IRect {
        self.bounds
    }
    pub fn width(&self) -> usize {
        self.bounds.width().max(0) as usize
    }
    pub fn height(&self) -> usize {
        self.bounds.height().max(0) as usize
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn get(&self, x: i32, y: i32) -> u8 {
        let b = self.bounds;
        if x < b.left || y < b.top || x >= b.right || y >= b.bottom {
            return 0;
        }
        self.data[(y - b.top) as usize * self.width() + (x - b.left) as usize]
    }
    /// The part of the mask inside `r` (outside the mask counts as 0).
    pub fn copy_rect(&self, r: IRect) -> Result<Self, ()> {
        let mut data = Vec::with_capacity(r.width().max(0) as usize * r.height().max(0) as usize);
        for y in r.top..r.bottom {
            for x in r.left..r.right {
                data.push(self.get(x, y));
            }
        }
        Self::from_raw(r, data)
    }
}

/// An RGBA image over a pixel rectangle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RgbaImage {
    bounds: IRect,
    data: Vec<[u8; 4]>,
}

impl RgbaImage {
    pub fn empty_at(x: i32, y: i32) -> Self {
        Self {
            bounds: IRect::new(x, y, x, y),
            data: Vec::new(),
        }
    }
    pub fn from_raw(bounds: IRect, data: Vec<[u8; 4]>) -> Result<Self, ()> {
        let (w, h) = (bounds.width().max(0) as usize, bounds.height().max(0) as usize);
        if data.len() == w * h {
            Ok(Self { bounds, data })
        } else {
            Err(())
        }
    }
    pub fn bounds(&self) -> IRect {
        self.bounds
    }
    pub fn width(&self) -> usize {
        self.bounds.width().max(0) as usize
    }
    pub fn height(&self) -> usize {
        self.bounds.height().max(0) as usize
    }
    pub fn data(&self) -> &[[u8; 4]] {
        &self.data
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// The smallest rectangle holding every pixel with any alpha.
    pub fn alpha_bounds(&self) -> IRect {
        let (w, h) = (self.width(), self.height());
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..h {
            let row = &self.data[y * w..(y + 1) * w];
            if let (Some(first), Some(last)) = (
                row.iter().position(|p| p[3] > 0),
                row.iter().rposition(|p| p[3] > 0),
            ) {
                x0 = x0.min(first);
                x1 = x1.max(last + 1);
                y0 = y0.min(y);
                y1 = y + 1;
            }
        }
        let (l, t) = (self.bounds.left, self.bounds.top);
        if x0 == usize::MAX {
            return IRect::new(l, t, l, t);
        }
        IRect::new(l + x0 as i32, t + y0 as i32, l + x1 as i32, t + y1 as i32)
    }
    /// The part of the image inside `r` (outside the image counts as transparent).
    pub fn copy_rect(&self, r: IRect) -> Result<Self, ()> {
        let b = self.bounds;
        let w = self.width();
        let mut data = Vec::with_capacity(r.width().max(0) as usize * r.height().max(0) as usize);
        for y in r.top..r.bottom {
            for x in r.left..r.right {
                let inside = x >= b.left && y >= b.top && x < b.right && y < b.bottom;
                data.push(if inside {
                    self.data[(y - b.top) as usize * w + (x - b.left) as usize]
                } else {
                    [0; 4]
                });
            }
        }
        Self::from_raw(r, data)
    }
}

/// A cubic segment's point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BezPoint {
    pub x: f64,
    pub y: f64,
}

/// A contour's cubic segment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CubicBez {
    pub p0: BezPoint,
    pub p1: BezPoint,
    pub p2: BezPoint,
    pub p3: BezPoint,
}

impl CubicBez {
    /// Whether both control points sit on their ends.
    pub fn is_line(&self) -> bool {
        self.p1 == self.p0 && self.p2 == self.p3
    }
    /// The point at `t` (0..=1).
    pub fn eval(&self, t: f64) -> BezPoint {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        BezPoint {
            x: a * self.p0.x + b * self.p1.x + c * self.p2.x + d * self.p3.x,
            y: a * self.p0.y + b * self.p1.y + c * self.p2.y + d * self.p3.y,
        }
    }
}

/// What the text engine needs of a contour beyond the shared type.
pub trait ContourExt {
    fn segments(&self) -> Vec<CubicBez>;
    fn reversed(&self) -> BezierContour;
}

impl ContourExt for BezierContour {
    fn segments(&self) -> Vec<CubicBez> {
        let at = |p: Point| BezPoint {
            x: f64::from(p.x),
            y: f64::from(p.y),
        };
        (0..self.segment_count())
            .filter_map(|i| self.segment_points(i))
            .map(|[a, b, c, d]| CubicBez {
                p0: at(a),
                p1: at(b),
                p2: at(c),
                p3: at(d),
            })
            .collect()
    }
    /// Reversed direction; a closed contour keeps its start node.
    fn reversed(&self) -> BezierContour {
        let mut nodes: Vec<BezierNode> = self.nodes.iter().rev().map(BezierNode::swapped).collect();
        if self.closed && !nodes.is_empty() {
            nodes.rotate_right(1);
        }
        BezierContour::new(nodes, self.closed)
    }
}
