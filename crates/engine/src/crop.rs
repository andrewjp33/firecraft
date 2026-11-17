//! Crop Document (the Crop tool, Edit > Crop Document) and Crop Image (Edit > Crop Selected
//! Bitmap), as Fireworks MX does them (measured 2026-11-16, docs/behavior/crop.md):
//!
//! * The canvas becomes the crop rectangle, which may reach past the old canvas (the new area
//!   shows the canvas color). Fireworks keeps every object's document coordinates and moves the
//!   canvas's top-left (root `XLCf`/`YLCf`); FireCraft keeps objects in canvas coordinates, so
//!   it moves the objects and the saved origin together and writes the same file.
//! * Objects wholly outside the rectangle are deleted, in every layer and frame.
//! * Bitmaps are trimmed to the rectangle (preference `CropDocCropsImages`, on by default);
//!   vector objects, text and symbol instances that cross its edge are kept whole.
//! * Crop Image trims one bitmap to the rectangle; it never grows the bitmap.

use firecraft_doc::{Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

use crate::{DocState, ElementRef};

/// A crop rectangle in canvas pixels: left, top, right, bottom (right and bottom exclusive).
pub type CropRect = (i64, i64, i64, i64);

/// The largest canvas side (as Modify > Canvas > Canvas Size).
const MAX_SIDE: i64 = 32_000;

/// Whether bounds `b` (x, y, w, h) and `r` share any area.
fn overlaps(b: (f64, f64, f64, f64), r: CropRect) -> bool {
    b.0 < r.2 as f64 && b.0 + b.2 > r.0 as f64 && b.1 < r.3 as f64 && b.1 + b.3 > r.1 as f64
}

/// Trim a bitmap's pixels to `r` (canvas pixels). `false` if nothing of it is left.
fn trim_to(b: &mut firecraft_doc::Bitmap, r: CropRect) -> bool {
    let (w, h) = (i64::from(b.image.width()), i64::from(b.image.height()));
    let (l, t) = (b.x.max(r.0), b.y.max(r.1));
    let (rr, bb) = ((b.x + w).min(r.2), (b.y + h).min(r.3));
    if rr <= l || bb <= t {
        return false;
    }
    if (l, t, rr, bb) != (b.x, b.y, b.x + w, b.y + h) {
        let mut img = RgbaImage::new((rr - l) as u32, (bb - t) as u32, Rgba::TRANSPARENT);
        img.copy_from(&b.image, b.x - l, b.y - t);
        b.image = img;
        b.x = l;
        b.y = t;
    }
    true
}

/// Crop Document on one object: `false` if it goes (wholly outside). Bitmaps, also those in
/// groups, are trimmed; a masked group's contents are left as they are (outside the canvas
/// they don't show).
fn crop_element(el: &mut Element, r: CropRect) -> bool {
    if el.bounds().is_some_and(|b| !overlaps(b, r)) {
        return false;
    }
    match &mut el.kind {
        ElementKind::Bitmap(b) => trim_to(b, r),
        ElementKind::Group(children) => {
            children.retain_mut(|c| crop_element(c, r));
            !children.is_empty()
        }
        _ => true,
    }
}

impl DocState {
    /// Crop the document to `r` (Fireworks MX's undo step "Crop Document"). Nothing happens for
    /// an empty or oversized rectangle, the whole canvas, or in the Symbol Editor.
    pub fn crop_document(&mut self, r: CropRect) -> bool {
        let (w, h) = (r.2 - r.0, r.3 - r.1);
        let whole = r == (0, 0, i64::from(self.doc.width), i64::from(self.doc.height));
        if w < 1 || h < 1 || w > MAX_SIDE || h > MAX_SIDE || whole || self.symbol_link.is_some() {
            return false;
        }
        self.edit("Crop Document", |s| {
            for layer in &mut s.doc.layers {
                for cell in &mut layer.cells {
                    cell.retain_mut(|el| crop_element(el, r));
                    for el in cell.iter_mut() {
                        el.translate(-r.0 as f64, -r.1 as f64);
                    }
                }
            }
            s.doc.width = w as u32;
            s.doc.height = h as u32;
            firecraft_io::shift_canvas_origin(&mut s.doc, r.0 as f64, r.1 as f64);
            // Objects went and indices moved; the pixel selection was anchored to the old canvas.
            s.selection.clear();
            s.pixels = None;
            s.bitmap_edit = None;
        });
        true
    }

    /// Edit > Crop Document: the rectangle it crops to at once, the selected pixels' bounds or
    /// else the selected objects' (rounded out to whole pixels). `None` with nothing selected
    /// (the command is disabled then, as in Fireworks MX).
    pub fn crop_document_rect(&self) -> Option<CropRect> {
        if let Some(p) = &self.pixels
            && let Some((x, y, w, h)) = p.mask.bounds()
        {
            let (l, t) = (p.x + i64::from(x), p.y + i64::from(y));
            return Some((l, t, l + i64::from(w), t + i64::from(h)));
        }
        let (x, y, w, h) = self.selection_bounds()?;
        const EPS: f64 = 1e-6;
        let r = (
            (x + EPS).floor() as i64,
            (y + EPS).floor() as i64,
            (x + w - EPS).ceil() as i64,
            (y + h - EPS).ceil() as i64,
        );
        (r.2 > r.0 && r.3 > r.1).then_some(r)
    }

    /// The bitmap Edit > Crop Selected Bitmap works on: the bitmap being edited, or the one
    /// selected bitmap.
    pub fn crop_bitmap_target(&self) -> Option<ElementRef> {
        let target = self.pixels.as_ref().and_then(|p| p.target).or(self.bitmap_edit);
        let r = match target {
            Some(r) => r,
            None => match self.selection.as_slice() {
                [r] => *r,
                _ => return None,
            },
        };
        matches!(self.element(r)?.kind, ElementKind::Bitmap(_)).then_some(r)
    }

    /// The canvas rectangle of bitmap `r`.
    pub fn bitmap_rect(&self, r: ElementRef) -> Option<CropRect> {
        match &self.element(r)?.kind {
            ElementKind::Bitmap(b) => Some((
                b.x,
                b.y,
                b.x + i64::from(b.image.width()),
                b.y + i64::from(b.image.height()),
            )),
            _ => None,
        }
    }

    /// Trim bitmap `r` to `rect` (Fireworks MX's undo step "Crop Image"). Nothing happens if
    /// `rect` leaves nothing of it or keeps all of it.
    pub fn crop_bitmap(&mut self, r: ElementRef, rect: CropRect) -> bool {
        let Some(old) = self.bitmap_rect(r) else {
            return false;
        };
        let keep = (old.0.max(rect.0), old.1.max(rect.1), old.2.min(rect.2), old.3.min(rect.3));
        if keep.2 <= keep.0 || keep.3 <= keep.1 || keep == old {
            return false;
        }
        self.edit("Crop Image", |s| {
            if let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = s.element_mut(r)
            {
                trim_to(b, keep);
            }
            s.pixels = None;
            s.bitmap_edit = None;
            s.selection = vec![r];
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{Bitmap, Document};

    use crate::ToolOptions;

    fn bitmap(x: i64, y: i64, w: u32, h: u32) -> Element {
        let mut img = RgbaImage::new(w, h, Rgba::TRANSPARENT);
        for py in 0..h {
            for px in 0..w {
                img.set(i64::from(px), i64::from(py), Rgba::new(px as u8, py as u8, 7, 255));
            }
        }
        Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img }))
    }

    /// The probe measured in Fireworks MX (crates/engine/examples/crop_probe.rs, cropped to
    /// (40, 30)-(250, 160)).
    fn probe() -> DocState {
        let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
        let o = ToolOptions {
            fill: Some(Rgba::new(220, 60, 40, 255)),
            stroke: None,
            ..Default::default()
        };
        d.add_shape((10.0, 10.0, 90.0, 70.0), false, &o);
        d.add_shape((255.0, 5.0, 295.0, 45.0), true, &o);
        d.add_shape((100.0, 50.0, 140.0, 80.0), false, &o);
        d.add_element(bitmap(150, 90, 120, 90), "Bitmap");
        d.add_element(bitmap(5, 175, 30, 20), "Bitmap");
        d.selection.clear();
        d
    }

    /// Objects' bounds, rounded (path bounds carry float noise).
    fn bounds(els: &[Element]) -> Vec<(f64, f64, f64, f64)> {
        let r = |v: f64| (v * 1e6).round() / 1e6;
        els.iter()
            .filter_map(Element::bounds)
            .map(|b| (r(b.0), r(b.1), r(b.2), r(b.3)))
            .collect()
    }

    #[test]
    fn crop_document_as_fireworks_mx() {
        let mut d = probe();
        assert!(d.crop_document((40, 30, 250, 160)));
        assert_eq!((d.doc.width, d.doc.height), (210, 130));
        // Fireworks MX kept the two rectangles and the bitmap (trimmed to 100x70); the ellipse
        // and the small bitmap, wholly outside, went.
        let els = d.doc.layers[0].elements(0);
        let kept = bounds(els);
        assert_eq!(
            kept,
            vec![(-30.0, -20.0, 80.0, 60.0), (60.0, 20.0, 40.0, 30.0), (110.0, 60.0, 100.0, 70.0)]
        );
        // The trimmed bitmap keeps its pixels where they were.
        let ElementKind::Bitmap(b) = &els[2].kind else { panic!() };
        assert_eq!(b.image.get(0, 0), Some(Rgba::new(0, 0, 7, 255)));
        assert_eq!(b.image.get(99, 69), Some(Rgba::new(99, 69, 7, 255)));
        // Saved as Fireworks MX saves it: objects at their old document coordinates, the
        // canvas origin moved.
        let (root, _) = firecraft_io::fireworks_tree(&d.doc);
        assert_eq!((root.float_of("XLC"), root.float_of("YLC")), (Some(40.0), Some(30.0)));
        let back = firecraft_io::open(&firecraft_io::save_fireworks_png(&d.doc).unwrap(), None).unwrap();
        assert_eq!(
            bounds(back.layers[0].elements(0)),
            kept
        );
        // One undo step.
        assert_eq!(d.history.labels().last(), Some("Crop Document"));
        d.undo();
        assert_eq!((d.doc.width, d.doc.height), (300, 200));
        assert_eq!(d.doc.layers[0].elements(0).len(), 5);
    }

    #[test]
    fn crop_past_the_canvas_grows_it_and_crops_again() {
        let mut d = probe();
        assert!(d.crop_document((40, 30, 330, 230)));
        assert_eq!((d.doc.width, d.doc.height), (290, 200));
        // Only the small bitmap (5..35 across) is wholly outside.
        assert_eq!(d.doc.layers[0].elements(0).len(), 4);
        assert!(d.crop_document((10, 10, 100, 100)));
        let (root, _) = firecraft_io::fireworks_tree(&d.doc);
        assert_eq!((root.float_of("XLC"), root.float_of("YLC")), (Some(50.0), Some(40.0)));
    }

    #[test]
    fn nothing_to_crop() {
        let mut d = probe();
        assert!(!d.crop_document((0, 0, 300, 200)));
        assert!(!d.crop_document((10, 10, 10, 50)));
        assert!(d.crop_document_rect().is_none());
    }

    #[test]
    fn crop_document_to_the_selection() {
        let mut d = probe();
        d.selection = vec![ElementRef::new(0, 3)];
        assert_eq!(d.crop_document_rect(), Some((150, 90, 270, 180)));
    }

    #[test]
    fn crop_image_trims_but_never_grows() {
        let mut d = probe();
        let r = ElementRef::new(0, 3);
        d.selection = vec![r];
        assert_eq!(d.crop_bitmap_target(), Some(r));
        // Fireworks MX: box (170, 105)-(270, 175) over the 150,90 120x90 bitmap left 170,105 100x70.
        assert!(d.crop_bitmap(r, (170, 105, 290, 200)));
        assert_eq!(d.bitmap_rect(r), Some((170, 105, 270, 180)));
        assert_eq!(d.history.labels().last(), Some("Crop Image"));
        assert!(!d.crop_bitmap(r, (0, 0, 300, 200)));
        assert!(!d.crop_bitmap(r, (0, 0, 20, 20)));
        d.selection = vec![ElementRef::new(0, 0)];
        assert_eq!(d.crop_bitmap_target(), None);
    }
}
