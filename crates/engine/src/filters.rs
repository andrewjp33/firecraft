//! The Filters menu: an effect applied for good to the selected objects' pixels, or only to the
//! selected pixels of a bitmap. Objects that aren't bitmaps are converted to bitmaps first (their
//! Live Effects drawn in), as Fireworks does after asking.

use firecraft_doc::{Bitmap, Element, ElementKind};
use firecraft_raster::effects::Effect;
use firecraft_raster::{Rgba, RgbaImage};

use crate::{DocState, ElementRef};

/// `effect` applied to `img` (a bitmap's pixels): the filters work within the bitmap's
/// rectangle; effects that reach beyond it (Gaussian Blur) grow the image. Returns the new image
/// and its offset from the old one.
pub fn filter_image(img: &RgbaImage, effect: &Effect, math: firecraft_raster::ColorMath) -> (RgbaImage, i64, i64) {
    let m = effect.margin();
    let (w, h) = (img.width() + 2 * m, img.height() + 2 * m);
    let mut buf = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    buf.copy_from(img, m as i64, m as i64);
    effect.apply_in(&mut buf, math, (m, m, img.width(), img.height()));
    let keep = firecraft_raster::filters::alpha_bounds(&buf)
        .map_or((m, m, img.width(), img.height()), |b| {
            firecraft_raster::filters::union(b, (m, m, img.width(), img.height()))
        });
    let mut out = RgbaImage::new(keep.2, keep.3, Rgba::TRANSPARENT);
    out.copy_from(&buf, -(keep.0 as i64), -(keep.1 as i64));
    (out, keep.0 as i64 - m as i64, keep.1 as i64 - m as i64)
}

impl DocState {
    /// Whether the Filters menu applies: something selected that has pixels.
    pub fn can_filter(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Whether filtering the selection would convert objects to bitmaps first.
    pub fn filter_converts(&self) -> bool {
        self.pixels.is_none()
            && self
                .selection
                .iter()
                .any(|r| self.element(*r).is_some_and(|e| !matches!(e.kind, ElementKind::Bitmap(_))))
    }

    /// The first selected bitmap's pixels as a filter would see them (for its popup's histogram):
    /// the selected pixels only, when there are some.
    pub fn filter_input(&self) -> Option<RgbaImage> {
        if let Some(sel) = &self.pixels
            && let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = self.element(sel.target)
        {
            return Some(b.image.extract_masked(&sel.mask));
        }
        let r = *self.selection.first()?;
        match &self.element(r)?.kind {
            ElementKind::Bitmap(b) => Some(b.image.clone()),
            _ => self.bake(&[(r, self.element(r)?.clone())]).into_iter().next().map(|x| x.2),
        }
    }

    /// Filters menu: apply `effect` for good (one undo step named `label`).
    pub fn apply_filter(&mut self, effect: &Effect, label: &str) {
        self.edit(label, |s| s.filter_now(effect));
    }

    /// A filter dialog's preview, during a live edit begun when it opened: the document as it
    /// was then, with `effect` applied. OK commits the live edit, Cancel cancels it.
    pub fn preview_filter(&mut self, effect: &Effect) {
        if let Some(e) = &self.live_edit {
            self.doc = e.before.clone();
        }
        self.filter_now(effect);
        self.touch();
    }

    fn filter_now(&mut self, effect: &Effect) {
        let math = self.doc.color_math;
        // Only the selected pixels: blend the filtered bitmap in through the selection.
        if let Some(sel) = self.pixels.clone() {
            let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = self.element(sel.target)
            else {
                return;
            };
            let mut filtered = b.image.clone();
            let region = firecraft_raster::filters::whole(&filtered);
            effect.apply_in(&mut filtered, math, region);
            let mut out = b.image.clone();
            for (i, (o, f)) in out.pixels_mut().iter_mut().zip(filtered.pixels()).enumerate() {
                let (x, y) = ((i as u32 % sel.mask.width()) as i64, (i as u32 / sel.mask.width()) as i64);
                let k = sel.mask.get(x, y) as u32;
                if k == 255 {
                    *o = *f;
                } else if k > 0 {
                    let mix = |a: u8, b: u8| ((a as u32 * (255 - k) + b as u32 * k + 127) / 255) as u8;
                    *o = Rgba::new(mix(o.r, f.r), mix(o.g, f.g), mix(o.b, f.b), mix(o.a, f.a));
                }
            }
            if let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = self.element_mut(sel.target)
            {
                b.image = out;
            }
            return;
        }
        let refs: Vec<ElementRef> = self.selection.clone();
        let mut changes: Vec<(ElementRef, Element)> = Vec::new();
        for r in refs {
            let Some(e) = self.element(r) else { continue };
            let mut new = match &e.kind {
                ElementKind::Bitmap(_) => e.clone(),
                // Converted to a bitmap of how it looks, its Live Effects drawn in.
                _ => {
                    let Some((_, _, img, x, y)) = self.bake(&[(r, e.clone())]).into_iter().next() else {
                        continue;
                    };
                    let mut b = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img }));
                    b.name = e.name.clone();
                    b.visible = e.visible;
                    b.opacity = e.opacity;
                    b.blend = e.blend;
                    b
                }
            };
            if let ElementKind::Bitmap(b) = &mut new.kind {
                let (img, dx, dy) = filter_image(&b.image, effect, math);
                b.image = img;
                b.x += dx;
                b.y += dy;
            }
            changes.push((r, new));
        }
        for (r, e) in changes {
            if let Some(slot) = self.element_mut(r) {
                *slot = e;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::Document;

    #[test]
    fn invert_a_bitmap_and_undo() {
        let mut d = DocState::new(Document::new(20, 20, 72.0, Some(Rgba::WHITE)));
        d.add_element(
            Element::new(ElementKind::Bitmap(Bitmap {
                x: 2,
                y: 3,
                image: RgbaImage::new(4, 4, Rgba::new(10, 20, 30, 255)),
            })),
            "Add",
        );
        d.selection = vec![ElementRef::new(0, 0)];
        d.apply_filter(&Effect::Invert, "Invert");
        let Some(ElementKind::Bitmap(b)) = d.element(ElementRef::new(0, 0)).map(|e| &e.kind) else {
            panic!("not a bitmap");
        };
        assert_eq!(b.image.get(1, 1), Some(Rgba::new(245, 235, 225, 255)));
        assert_eq!((b.x, b.y), (2, 3));
    }

    #[test]
    fn gaussian_blur_grows_the_bitmap() {
        let img = RgbaImage::new(4, 4, Rgba::new(10, 20, 30, 255));
        let (out, dx, dy) = filter_image(&img, &Effect::GaussianBlur { radius: 2.0 }, Default::default());
        assert!(out.width() > 4 && dx < 0 && dy < 0);
    }
}
