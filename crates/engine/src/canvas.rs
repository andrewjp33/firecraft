//! Modify > Canvas > Canvas Size and Image Size, as Fireworks MX does them (measured
//! 2026-11-16, docs/behavior/canvas-size.md).

use firecraft_doc::{Element, ElementKind};
use firecraft_raster::transform::{Affine, Resample, resize_image};

use crate::DocState;
use crate::crop::MAX_SIDE;

/// Where Canvas Size keeps the old canvas: one of the 3x3 anchor buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Row by row, as the dialog shows them.
    pub const ALL: [Anchor; 9] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
    ];

    /// Column and row: -1 left/top, 0 middle, 1 right/bottom.
    pub fn sides(self) -> (i8, i8) {
        let i = Self::ALL.iter().position(|a| *a == self).unwrap_or(4) as i8;
        (i % 3 - 1, i / 3 - 1)
    }
}

/// How much of a size change goes before the old canvas (left or top). Fireworks MX splits an
/// odd change towards the left/top when growing and the right/bottom when shrinking: +101 put
/// 51 on the left, -101 took 50 off it, so ceil(change / 2).
fn before(change: i64, side: i8) -> i64 {
    match side {
        -1 => 0,
        1 => change,
        _ => change - change.div_euclid(2),
    }
}

/// Image Size's scaling of one object.
struct Fit {
    /// Canvas -> new canvas.
    m: Affine,
    scale: (f64, f64),
    /// Canvas origin in document coordinates, before and after.
    from: (f64, f64),
    to: (f64, f64),
    method: Resample,
}

impl Fit {
    /// Bitmaps land where Fireworks MX puts them, in document coordinates: the position
    /// rounded, the size rounded up (150x90 120x90 at x1.50125, x1.50192: 225,135 181x136; 5,175
    /// 30x20: 8,263 46x31). Everything else is transformed.
    fn apply(&self, el: &mut Element) {
        match &mut el.kind {
            ElementKind::Bitmap(b) => {
                const EPS: f64 = 1e-9;
                let (sx, sy) = self.scale;
                let x = ((b.x as f64 + self.from.0) * sx).round();
                let y = ((b.y as f64 + self.from.1) * sy).round();
                let w = (f64::from(b.image.width()) * sx - EPS).ceil().max(1.0) as u32;
                let h = (f64::from(b.image.height()) * sy - EPS).ceil().max(1.0) as u32;
                b.image = resize_image(&b.image, w, h, self.method);
                b.x = (x - self.to.0) as i64;
                b.y = (y - self.to.1) as i64;
            }
            ElementKind::Group(children) => children.iter_mut().for_each(|c| self.apply(c)),
            ElementKind::Masked { content, mask, cache, .. } => {
                content.iter_mut().for_each(|c| self.apply(c));
                self.apply(mask);
                if let Some(c) = cache {
                    c.transform_with(&self.m, self.method);
                }
            }
            _ => el.transform_with(&self.m, self.method),
        }
    }
}

impl DocState {
    /// The canvas rectangle Canvas Size makes (in current canvas pixels).
    pub fn canvas_size_rect(&self, w: u32, h: u32, anchor: Anchor) -> crate::CropRect {
        let (sx, sy) = anchor.sides();
        let dw = i64::from(w) - i64::from(self.doc.width);
        let dh = i64::from(h) - i64::from(self.doc.height);
        let (l, t) = (-before(dw, sx), -before(dh, sy));
        (l, t, l + i64::from(w), t + i64::from(h))
    }

    /// Modify > Canvas > Canvas Size: a crop to the anchored rectangle (Fireworks MX deletes
    /// objects left wholly outside and trims bitmaps, and names the undo step "Crop Document").
    pub fn canvas_size(&mut self, w: u32, h: u32, anchor: Anchor) -> bool {
        let r = self.canvas_size_rect(w, h, anchor);
        self.crop_to(r)
    }

    /// Modify > Canvas > Image Size: the document becomes `w` x `h` pixels (everything scaled
    /// with it, bitmaps resampled with `method`) at `resolution` (pixels per inch, or per cm
    /// with `cm`). Fireworks MX scales about its document origin, so the saved canvas origin
    /// scales too (rounded): -1 at 150% became -2. Undo step "Image Size".
    pub fn image_size(&mut self, w: u32, h: u32, resolution: f64, cm: bool, method: Resample) -> bool {
        let (ow, oh) = (self.doc.width, self.doc.height);
        let resized = (w, h) != (ow, oh);
        let same_res = resolution == self.doc.resolution && cm == self.doc.resolution_cm;
        let bad = w == 0 || h == 0 || i64::from(w) > MAX_SIDE || i64::from(h) > MAX_SIDE;
        if bad || ow == 0 || oh == 0 || !resolution.is_finite() || resolution <= 0.0 || (!resized && same_res) {
            return false;
        }
        self.edit("Image Size", |s| {
            s.doc.resolution = resolution;
            s.doc.resolution_cm = cm;
            if !resized {
                return;
            }
            let (sx, sy) = (f64::from(w) / f64::from(ow), f64::from(h) / f64::from(oh));
            let o = firecraft_io::canvas_origin(&s.doc);
            let n = ((o.0 * sx).round(), (o.1 * sy).round());
            // Canvas coordinates -> document, scale, -> the new canvas.
            let scale = Affine {
                a: sx,
                d: sy,
                ..Affine::IDENTITY
            };
            let m = Affine::translate(-n.0, -n.1).then(&scale.then(&Affine::translate(o.0, o.1)));
            let fit = Fit { m, scale: (sx, sy), from: o, to: n, method };
            for layer in &mut s.doc.layers {
                for cell in &mut layer.cells {
                    for el in cell.iter_mut() {
                        fit.apply(el);
                    }
                }
            }
            s.doc.width = w;
            s.doc.height = h;
            firecraft_io::shift_canvas_origin(&mut s.doc, n.0 - o.0, n.1 - o.1);
            s.selection.clear();
            s.pixels = None;
            s.bitmap_edit = None;
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{Document, Element};
    use firecraft_raster::Rgba;

    use crate::ToolOptions;

    fn doc(w: u32, h: u32) -> DocState {
        let mut d = DocState::new(Document::new(w, h, 72.0, Some(Rgba::WHITE)));
        let o = ToolOptions {
            fill: Some(Rgba::new(220, 60, 40, 255)),
            stroke: None,
            ..Default::default()
        };
        d.add_shape((10.0, 10.0, 90.0, 70.0), false, &o);
        d.selection.clear();
        d
    }

    fn origin(d: &DocState) -> (Option<f64>, Option<f64>) {
        let (root, _) = firecraft_io::fireworks_tree(&d.doc);
        (root.float_of("XLC"), root.float_of("YLC"))
    }

    fn bounds(d: &DocState) -> (f64, f64, f64, f64) {
        let b = d.doc.layers[0].elements(0)[0].bounds().unwrap();
        let r = |v: f64| (v * 1e6).round() / 1e6;
        (r(b.0), r(b.1), r(b.2), r(b.3))
    }

    #[test]
    fn centre_anchor_splits_odd_changes_as_fireworks_mx() {
        // 300x200 -> 401x261, centre: origin -51, -31 (Fireworks MX).
        let mut d = doc(300, 200);
        assert!(d.canvas_size(401, 261, Anchor::Center));
        assert_eq!(origin(&d), (Some(-51.0), Some(-31.0)));
        assert_eq!(d.history.labels().last(), Some("Crop Document"));
        // Back to 300x200: origin -1, -1.
        assert!(d.canvas_size(300, 200, Anchor::Center));
        assert_eq!(origin(&d), (Some(-1.0), Some(-1.0)));
        // The rectangle stays where it was in the document: 10,10 -> canvas 11,11.
        assert_eq!(bounds(&d), (11.0, 11.0, 80.0, 60.0));
    }

    #[test]
    fn anchors() {
        let d = doc(300, 200);
        assert_eq!(d.canvas_size_rect(400, 260, Anchor::TopLeft), (0, 0, 400, 260));
        assert_eq!(d.canvas_size_rect(400, 260, Anchor::BottomRight), (-100, -60, 300, 200));
        assert_eq!(d.canvas_size_rect(160, 100, Anchor::Right), (140, 50, 300, 150));
        assert_eq!(d.canvas_size_rect(160, 100, Anchor::Bottom), (70, 100, 230, 200));
    }

    #[test]
    fn shrinking_deletes_what_is_left_outside() {
        let mut d = doc(300, 200);
        let o = ToolOptions {
            fill: Some(Rgba::new(0, 0, 255, 255)),
            stroke: None,
            ..Default::default()
        };
        d.add_shape((200.0, 120.0, 260.0, 180.0), true, &o);
        assert!(d.canvas_size(160, 100, Anchor::TopLeft));
        assert_eq!(d.doc.layers[0].elements(0).len(), 1);
        assert_eq!(origin(&d), (Some(0.0), Some(0.0)));
    }

    #[test]
    fn image_size_scales_about_the_document_origin() {
        // Fireworks MX: a 160x100 canvas at origin -1,-1 with a rectangle at document 10,10
        // 80x60, Image Size at 108 ppi (x1.5): 240x150, origin -2,-2, rectangle 15,15 120x90.
        let mut d = doc(300, 200);
        d.canvas_size(300, 200, Anchor::TopLeft);
        d.canvas_size(301, 201, Anchor::BottomRight); // origin -1, -1
        d.canvas_size(160, 100, Anchor::TopLeft);
        assert_eq!(origin(&d), (Some(-1.0), Some(-1.0)));
        assert!(d.image_size(240, 150, 108.0, false, Resample::Bicubic));
        assert_eq!((d.doc.width, d.doc.height, d.doc.resolution), (240, 150, 108.0));
        assert_eq!(origin(&d), (Some(-2.0), Some(-2.0)));
        // Document 15,15 is canvas 17,17.
        assert_eq!(bounds(&d), (17.0, 17.0, 120.0, 90.0));
        assert_eq!(d.history.labels().last(), Some("Image Size"));
        // Resolution only.
        assert!(d.image_size(240, 150, 40.0, true, Resample::Bicubic));
        assert_eq!((d.doc.width, d.doc.resolution, d.doc.resolution_cm), (240, 40.0, true));
        assert_eq!(bounds(&d), (17.0, 17.0, 120.0, 90.0));
        assert!(!d.image_size(240, 150, 40.0, true, Resample::Bicubic));
    }

    #[test]
    fn image_size_places_bitmaps_as_fireworks_mx() {
        use firecraft_doc::{Bitmap, ElementKind};
        use firecraft_raster::RgbaImage;
        // Fireworks MX: crop_probe, Canvas Size 401x261 (centre) then Image Size 602x392:
        // the 30x20 bitmap at document 5,175 went to 8,263 46x31, the 120x90 one at 150,90 to
        // 225,135 181x136, origin -77,-47.
        let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
        for (x, y, w, h) in [(5, 175, 30, 20), (150, 90, 120, 90)] {
            let img = RgbaImage::new(w, h, Rgba::new(9, 9, 9, 255));
            d.add_element(Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img })), "Bitmap");
        }
        assert!(d.canvas_size(401, 261, Anchor::Center));
        assert!(d.image_size(602, 392, 108.0, false, Resample::Bicubic));
        assert_eq!(origin(&d), (Some(-77.0), Some(-47.0)));
        let placed: Vec<_> = d.doc.layers[0]
            .elements(0)
            .iter()
            .map(|e| match &e.kind {
                ElementKind::Bitmap(b) => (b.x - 77, b.y - 47, b.image.width(), b.image.height()),
                _ => panic!(),
            })
            .collect();
        assert_eq!(placed, vec![(8, 263, 46, 31), (225, 135, 181, 136)]);
    }

    #[test]
    fn image_size_resamples_bitmaps() {
        use firecraft_doc::{Bitmap, ElementKind};
        use firecraft_raster::RgbaImage;
        for method in Resample::ALL {
            let mut d = DocState::new(Document::new(10, 10, 72.0, None));
            let img = RgbaImage::new(10, 10, Rgba::new(10, 200, 30, 255));
            d.add_element(Element::new(ElementKind::Bitmap(Bitmap { x: 0, y: 0, image: img })), "Bitmap");
            assert!(d.image_size(20, 20, 72.0, false, method));
            let ElementKind::Bitmap(b) = &d.doc.layers[0].elements(0)[0].kind else { panic!() };
            assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (0, 0, 20, 20), "{method:?}");
            assert_eq!(b.image.get(10, 10), Some(Rgba::new(10, 200, 30, 255)), "{method:?}");
        }
    }
}
