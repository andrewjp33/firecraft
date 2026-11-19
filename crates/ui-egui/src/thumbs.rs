//! Layers panel thumbnails, drawn as Fireworks CS draws them (measured by FirePixel, its RE spec
//! 73): the object at its place on the page reduced to fit the thumbnail, checkered where it is
//! transparent; a masked object's content alone; a mask grey on black (its brightness times its
//! alpha for a grayscale mask, its alpha otherwise). They follow the document but not during a
//! drag or stroke: a thumbnail whose object is changing keeps its old picture until the mouse is
//! released.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, vec2};
use firecraft_doc::{Element, MaskMode, RenderCtx};
use firecraft_engine::ElementRef;
use firecraft_raster::{Rgba, RgbaImage};

/// The thumbnail frame (1 px border around the picture), as in Fireworks CS (34 x 23).
pub const FRAME: egui::Vec2 = vec2(34.0, 23.0);

/// How long thumbnails may take to draw in one frame; the rest follow in later frames.
const BUDGET: Duration = Duration::from_millis(12);

/// Which picture of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Which {
    /// The object itself (or a masked object's content).
    Object,
    /// A masked object's mask.
    Mask,
}

#[derive(Clone)]
struct Entry {
    key: u64,
    /// The document revision `key` was last checked at (unchanged since: no need to check).
    checked: u64,
    tex: egui::TextureHandle,
    used: u64,
}

/// Thumbnails drawn so far, per document, row and picture.
#[derive(Clone, Default)]
pub struct Cache {
    map: HashMap<(u64, ElementRef, Which), Entry>,
    tick: u64,
    deadline: Option<Instant>,
    pending: bool,
    /// Whether a thumbnail was drawn this frame (one always is, however long the frame took).
    drawn: bool,
}

impl Cache {
    /// Start a frame of the Layers panel.
    pub fn begin(&mut self) {
        self.tick += 1;
        self.deadline = Some(Instant::now() + BUDGET);
        self.pending = false;
        self.drawn = false;
    }

    /// End it: forget thumbnails of rows not shown for a while; true when some thumbnails
    /// still have to be drawn (ask for another frame).
    pub fn end(&mut self) -> bool {
        let tick = self.tick;
        self.map.retain(|_, e| tick.saturating_sub(e.used) < 600);
        self.pending
    }
}

/// What a thumbnail shows.
pub struct Subject<'a> {
    /// The objects drawn (a masked object's content, or the object).
    pub elements: &'a [Element],
    /// `Some` for a mask: drawn grey on black by this mode.
    pub mask: Option<MaskMode>,
}

/// Where thumbnails are drawn from: the page and how to render.
pub struct Page<'a> {
    pub doc_id: u64,
    /// The document's revision (thumbnails are checked again only when it changed).
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub ctx: RenderCtx<'a>,
}

/// Draw a row's thumbnail (allocating its frame), rendering it if the object changed since
/// it was last drawn and there is time left in this frame. `selected` frames it in the
/// selection colour, `mask_edit` in Fireworks CS's green for the mask being edited.
pub fn show(
    ui: &mut egui::Ui,
    cache: &mut Cache,
    page: &Page,
    slot: (ElementRef, Which),
    subject: &Subject,
    mask_edit: bool,
) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(FRAME, Sense::hover());
    let id = (page.doc_id, slot.0, slot.1);
    // The fingerprint is checked only after the document changed (hashing every row's objects
    // each frame would be slow).
    let known = cache.map.get(&id).map(|e| (e.key, e.checked));
    let held = ui.input(|i| i.pointer.any_down());
    let key = match known {
        Some((k, checked)) if checked == page.revision => k,
        // While the mouse is held (a drag or a stroke) thumbnails keep their pictures: no
        // need to look for changes until it is released.
        Some((k, _)) if held => k,
        _ => subject_key(subject, page),
    };
    if let Some(e) = cache.map.get_mut(&id)
        && e.key == key
        && !held
    {
        e.checked = page.revision;
    }
    let stale = known.is_none_or(|(k, _)| k != key);
    // While the mouse is held (a drag or a stroke), a changed object keeps its old picture.
    let keep_old = stale && held && cache.map.contains_key(&id);
    if stale && !keep_old {
        if !cache.drawn || cache.deadline.is_some_and(|d| Instant::now() < d) {
            cache.drawn = true;
            let img = render(subject, page, r.size() - vec2(2.0, 2.0));
            let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img.to_rgba_bytes());
            let tex = ui.ctx().load_texture(
                format!("layer_thumb_{}_{:?}_{:?}", page.doc_id, slot.0, slot.1),
                color,
                egui::TextureOptions::LINEAR,
            );
            cache.map.insert(
                id,
                Entry {
                    key,
                    checked: page.revision,
                    tex,
                    used: cache.tick,
                },
            );
        } else {
            cache.pending = true;
        }
    }
    let p = ui.painter();
    let inner = r.shrink(1.0);
    // The page fitted inside: checkered (transparent) for objects, black for masks.
    let pr = page_rect(inner, page);
    if subject.mask.is_some() {
        p.rect_filled(inner, 0.0, Color32::BLACK);
    } else {
        p.rect_filled(inner, 0.0, Color32::WHITE);
        checker(p, pr);
    }
    if let Some(e) = cache.map.get_mut(&id) {
        e.used = cache.tick;
        p.image(e.tex.id(), inner, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
    if mask_edit {
        p.rect_stroke(r, 0.0, Stroke::new(2.0, crate::panels::MASK_EDIT_GREEN), StrokeKind::Outside);
    } else {
        p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(0x69)), StrokeKind::Inside);
    }
    resp
}

/// Where the page sits in a thumbnail's picture area (fitted, centred).
fn page_rect(inner: Rect, page: &Page) -> Rect {
    let (w, h) = (page.width.max(1) as f32, page.height.max(1) as f32);
    let s = (inner.width() / w).min(inner.height() / h);
    Rect::from_center_size(inner.center(), vec2(w * s, h * s))
}

fn checker(p: &egui::Painter, r: Rect) {
    const CELL: f32 = 3.0;
    p.rect_filled(r, 0.0, Color32::WHITE);
    let (nx, ny) = ((r.width() / CELL).ceil() as i32, (r.height() / CELL).ceil() as i32);
    for j in 0..ny {
        for i in (j % 2..nx).step_by(2) {
            let min = r.min + vec2(i as f32 * CELL, j as f32 * CELL);
            let cell = Rect::from_min_size(min, vec2(CELL, CELL)).intersect(r);
            p.rect_filled(cell, 0.0, Color32::from_gray(0xCC));
        }
    }
}

fn subject_key(subject: &Subject, page: &Page) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in subject.elements {
        firecraft_doc::element_key(e, &page.ctx).hash(&mut h);
        (e.opacity.to_bits(), e.blend.fw_code()).hash(&mut h);
    }
    format!("{:?}", subject.mask).hash(&mut h);
    (page.width, page.height, page.ctx.frame).hash(&mut h);
    h.finish()
}

/// The thumbnail picture (`size` = the picture area in points, one pixel per point): the
/// subject at its place on the page, averaged down, transparent around it.
fn render(subject: &Subject, page: &Page, size: egui::Vec2) -> RgbaImage {
    let (tw, th) = (size.x.max(1.0) as u32, size.y.max(1.0) as u32);
    let mut out = RgbaImage::new(tw, th, Rgba::TRANSPARENT);
    let (pw, ph) = (page.width.max(1) as f64, page.height.max(1) as f64);
    let s = (tw as f64 / pw).min(th as f64 / ph);
    // The page's place in the picture.
    let (ox, oy) = ((tw as f64 - pw * s) / 2.0, (th as f64 - ph * s) / 2.0);
    // Sums of premultiplied colour per thumbnail pixel.
    let mut acc = vec![[0f64; 4]; (tw * th) as usize];
    for el in subject.elements {
        // A hidden object still shows in its thumbnail.
        let shown;
        let el = if el.visible && el.opacity > 0.0 {
            el
        } else {
            shown = Element { visible: true, opacity: if el.opacity > 0.0 { el.opacity } else { 100.0 }, ..el.clone() };
            &shown
        };
        // Only the part on the page counts (thumbnails show the page).
        let Some((rx, ry, rw, rh)) = el.full_region(&page.ctx) else { continue };
        let (x0, y0) = (rx.max(0), ry.max(0));
        let (x1, y1) = ((rx + rw as i64).min(page.width as i64), (ry + rh as i64).min(page.height as i64));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let mut img = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32, Rgba::TRANSPARENT);
        el.render_onto(&mut img, x0, y0, &page.ctx);
        for y in 0..img.height() as i64 {
            let ty = oy + (y0 + y) as f64 * s;
            for x in 0..img.width() as i64 {
                let Some(px) = img.get(x, y) else { continue };
                if px.a == 0 {
                    continue;
                }
                let tx = ox + (x0 + x) as f64 * s;
                // The thumbnail pixels this page pixel lands on (one, or a block on a page
                // smaller than the thumbnail).
                let (cx0, cy0) = (tx as i64, ty as i64);
                let (cx1, cy1) = ((tx + s) as i64, (ty + s) as i64);
                let (cx1, cy1) = (cx1.max(cx0 + 1), cy1.max(cy0 + 1));
                let a = px.a as f64 / 255.0;
                let (r, g, b) = match subject.mask {
                    // A mask: how much it reveals, as grey.
                    Some(mode) => {
                        let v = match mode {
                            MaskMode::Grayscale => (0.299 * px.r as f64 + 0.587 * px.g as f64 + 0.114 * px.b as f64) * a,
                            MaskMode::Alpha | MaskMode::PathOutline => 255.0 * a,
                        };
                        (v, v, v)
                    }
                    None => (px.r as f64 * a, px.g as f64 * a, px.b as f64 * a),
                };
                for cy in cy0.max(0)..cy1.min(th as i64) {
                    for cx in cx0.max(0)..cx1.min(tw as i64) {
                        if let Some(c) = acc.get_mut((cy * tw as i64 + cx) as usize) {
                            c[0] += r;
                            c[1] += g;
                            c[2] += b;
                            c[3] += if subject.mask.is_some() { 255.0 } else { a * 255.0 };
                        }
                    }
                }
            }
        }
    }
    // Each thumbnail pixel covers 1/s^2 page pixels.
    let cover = (1.0 / (s * s)).max(1.0);
    for (i, c) in acc.iter().enumerate() {
        let (x, y) = ((i as u32 % tw) as i64, (i as u32 / tw) as i64);
        if subject.mask.is_some() {
            // Grey on black, opaque.
            let v = (c[0] / cover).round().clamp(0.0, 255.0) as u8;
            if c[3] > 0.0 {
                out.set(x, y, Rgba::new(v, v, v, 255));
            }
            continue;
        }
        let a = (c[3] / cover).clamp(0.0, 255.0);
        if a <= 0.0 {
            continue;
        }
        let un = |v: f64| (v / cover * 255.0 / a).round().clamp(0.0, 255.0) as u8;
        out.set(x, y, Rgba::new(un(c[0]), un(c[1]), un(c[2]), a.round() as u8));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{Bitmap, ElementKind};

    fn page() -> Page<'static> {
        Page {
            doc_id: 1,
            revision: 0,
            width: 64,
            height: 42,
            ctx: RenderCtx::EMPTY,
        }
    }

    /// A bitmap of `c` over the page's left half.
    fn left_half(c: Rgba) -> Element {
        Element::new(ElementKind::Bitmap(Bitmap {
            x: 0,
            y: 0,
            image: RgbaImage::new(32, 42, c),
        }))
    }

    #[test]
    fn an_object_shows_at_its_place_on_the_page() {
        let el = [left_half(Rgba::new(200, 0, 0, 255))];
        let img = render(&Subject { elements: &el, mask: None }, &page(), vec2(32.0, 21.0));
        assert_eq!((img.width(), img.height()), (32, 21));
        assert_eq!(img.get(4, 10), Some(Rgba::new(200, 0, 0, 255)));
        assert_eq!(img.get(28, 10).map(|p| p.a), Some(0));
    }

    #[test]
    fn a_hidden_object_still_shows() {
        let mut el = left_half(Rgba::new(0, 0, 200, 255));
        el.visible = false;
        let img = render(&Subject { elements: &[el], mask: None }, &page(), vec2(32.0, 21.0));
        assert_eq!(img.get(4, 10), Some(Rgba::new(0, 0, 200, 255)));
    }

    #[test]
    fn a_mask_is_grey_by_what_it_reveals() {
        // Mid grey reveals half (grayscale mask), so its thumbnail is mid grey; nothing drawn
        // stays clear (black shows from the frame).
        let el = [left_half(Rgba::new(128, 128, 128, 255))];
        let gray = render(
            &Subject { elements: &el, mask: Some(MaskMode::Grayscale) },
            &page(),
            vec2(32.0, 21.0),
        );
        assert_eq!(gray.get(4, 10), Some(Rgba::new(128, 128, 128, 255)));
        assert_eq!(gray.get(28, 10).map(|p| p.a), Some(0));
        // By alpha, an opaque grey reveals all.
        let alpha = render(&Subject { elements: &el, mask: Some(MaskMode::Alpha) }, &page(), vec2(32.0, 21.0));
        assert_eq!(alpha.get(4, 10), Some(Rgba::new(255, 255, 255, 255)));
    }
}
