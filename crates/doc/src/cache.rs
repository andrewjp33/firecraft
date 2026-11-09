//! Per-object render cache.
//!
//! Each top-level object's pixels (after its effects, before opacity and blending) are kept,
//! keyed by a fingerprint of the object *moved to its whole-pixel anchor* (the floor of its
//! bounds' top-left). Moving an object therefore finds its pixels in the cache and only
//! re-blends them at the new place; only objects whose content or effects changed are
//! rendered again. Pass-through groups are entered so their children are cached individually;
//! pass-through symbol instances are cached whole (their children are resampled copies).
//! Least recently used entries are dropped past a memory budget.
//!
//! Refreshes are incremental too: when only some objects changed (moved, edited, shown or
//! hidden), just the area they covered before and cover now is composited again, into the
//! previous picture, and [`RenderCache::dirty`] says which area that was.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use firecraft_raster::{BlendMode, Rgba, RgbaImage};

use crate::effects::EffectParam;
use crate::element::{Element, ElementKind, Text};
use crate::{Document, RenderCtx};

/// Which part of the picture the last [`Document::render_cached`] changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dirty {
    /// All of it (first render, other frame, size or canvas change, objects added/removed).
    #[default]
    Full,
    /// Nothing.
    None,
    /// This rectangle (canvas pixels).
    Rect { x: u32, y: u32, w: u32, h: u32 },
}

#[derive(Default)]
pub struct RenderCache {
    entries: HashMap<u64, Entry>,
    bytes: usize,
    clock: u64,
    /// Total bytes kept before evicting (default 512 MB).
    pub budget: usize,
    /// Statistics of the last render: (cache hits, objects rendered).
    pub last: (usize, usize),
    /// What the last render changed.
    pub dirty: Dirty,
    /// The last render: its settings, what it was made of, and the picture.
    previous: Option<(u64, Vec<UnitSig>, RgbaImage)>,
}

struct Entry {
    pieces: Vec<Piece>,
    used: u64,
}

/// One rendered object: a plain object is one piece; a pass-through symbol instance is one per
/// (resampled) child, each keeping its own opacity and blend mode.
struct Piece {
    image: Arc<RgbaImage>,
    /// Position relative to the anchor.
    dx: i64,
    dy: i64,
    /// For an instance's children; `None` uses the object's own opacity and blend mode.
    blend: Option<(u8, BlendMode)>,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.pieces.iter().map(|p| p.image.pixels().len() * 4).sum()
    }
}

/// Something drawn in a render, in order.
enum Unit<'a> {
    /// A cached object at its anchor.
    Cached {
        key: u64,
        ax: i64,
        ay: i64,
        opacity: u8,
        blend: BlendMode,
    },
    /// An object rendered directly each time (unbounded or huge).
    Direct { el: Box<Element>, ctx: RenderCtx<'a> },
}

/// A unit's identity (for finding what changed) and the area it covers (x0, y0, x1, y1);
/// `None` for an unknown area.
#[derive(Clone, Copy, PartialEq)]
struct UnitSig {
    sig: u64,
    rect: Option<(i64, i64, i64, i64)>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            budget: 512 << 20,
            ..Default::default()
        }
    }

    fn evict(&mut self) {
        let budget = if self.budget == 0 { 512 << 20 } else { self.budget };
        while self.bytes > budget {
            let Some((&k, _)) = self.entries.iter().min_by_key(|(_, e)| e.used) else {
                break;
            };
            if let Some(e) = self.entries.remove(&k) {
                self.bytes = self.bytes.saturating_sub(e.bytes());
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.previous = None;
    }

    fn sig_of(&self, u: &Unit) -> UnitSig {
        let mut h = DefaultHasher::new();
        match u {
            Unit::Cached {
                key,
                ax,
                ay,
                opacity,
                blend,
            } => {
                (key, ax, ay, opacity, blend.fw_code()).hash(&mut h);
                let mut r = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
                for p in self.entries.get(key).map_or(&[][..], |e| &e.pieces[..]) {
                    let (x, y) = (ax + p.dx, ay + p.dy);
                    r = (
                        r.0.min(x),
                        r.1.min(y),
                        r.2.max(x + p.image.width() as i64),
                        r.3.max(y + p.image.height() as i64),
                    );
                }
                UnitSig {
                    sig: h.finish(),
                    rect: Some(r),
                }
            }
            Unit::Direct { el, ctx } => {
                (fingerprint(el, ctx), el.opacity.to_bits(), el.blend.fw_code()).hash(&mut h);
                UnitSig {
                    sig: h.finish(),
                    rect: None,
                }
            }
        }
    }

    /// Composite `units` onto `out`, whose top-left is canvas (`ox`, `oy`).
    fn draw(&self, units: &[Unit], out: &mut RgbaImage, ox: i64, oy: i64, math: firecraft_raster::ColorMath) {
        for u in units {
            match u {
                Unit::Cached {
                    key,
                    ax,
                    ay,
                    opacity,
                    blend,
                } => {
                    for p in self.entries.get(key).map_or(&[][..], |e| &e.pieces[..]) {
                        let (o, b) = p.blend.unwrap_or((*opacity, *blend));
                        out.draw_blend_with(&p.image, ax + p.dx - ox, ay + p.dy - oy, o, b, math);
                    }
                }
                Unit::Direct { el, ctx } => el.render_onto(out, ox, oy, ctx),
            }
        }
    }
}

/// Union of non-empty rectangles (x0, y0, x1, y1).
fn union(a: Option<(i64, i64, i64, i64)>, b: (i64, i64, i64, i64)) -> Option<(i64, i64, i64, i64)> {
    if b.2 <= b.0 || b.3 <= b.1 {
        return a;
    }
    Some(match a {
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        None => b,
    })
}

impl Document {
    /// [`render`](Document::render) using (and filling) `cache`.
    pub fn render_cached(&self, frame: usize, cache: &mut RenderCache) -> RgbaImage {
        let ctx = RenderCtx {
            mx_perspective: self.mx_perspective,
            symbols: &self.symbols,
            frame,
            depth: 0,
            math: self.color_math,
            chain: [0; crate::MAX_SYMBOL_DEPTH as usize],
        };
        let canvas = self.canvas.unwrap_or(Rgba::TRANSPARENT);
        cache.last = (0, 0);
        cache.clock += 1;
        let mut units = Vec::new();
        for layer in self.layers.iter().filter(|l| l.visible_in(frame)) {
            for el in layer.elements(frame) {
                collect_units(el, &ctx, cache, &mut units);
            }
        }
        let base = {
            let mut h = DefaultHasher::new();
            (self.width, self.height, canvas, frame, self.color_math as u8).hash(&mut h);
            h.finish()
        };
        let sigs: Vec<UnitSig> = units.iter().map(|u| cache.sig_of(u)).collect();
        let dirty = match &cache.previous {
            Some((b, prev, _)) if *b == base && prev.len() == sigs.len() => {
                let mut area = None;
                let mut full = false;
                for (p, c) in prev.iter().zip(&sigs).filter(|(p, c)| p != c) {
                    match (p.rect, c.rect) {
                        (Some(a), Some(b)) => area = union(union(area, a), b),
                        _ => {
                            full = true;
                            break;
                        }
                    }
                }
                let clipped = area.map(|r| {
                    (
                        r.0.max(0),
                        r.1.max(0),
                        r.2.min(self.width as i64),
                        r.3.min(self.height as i64),
                    )
                });
                match clipped {
                    _ if full => Dirty::Full,
                    Some((x0, y0, x1, y1)) if x1 > x0 && y1 > y0 => Dirty::Rect {
                        x: x0 as u32,
                        y: y0 as u32,
                        w: (x1 - x0) as u32,
                        h: (y1 - y0) as u32,
                    },
                    _ => Dirty::None,
                }
            }
            _ => Dirty::Full,
        };
        let previous = cache.previous.take().map(|(_, _, img)| img);
        let out = match (dirty, previous) {
            (Dirty::None, Some(img)) => img,
            (Dirty::Rect { x, y, w, h }, Some(mut img)) => {
                let mut part = RgbaImage::new(w, h, canvas);
                cache.draw(&units, &mut part, x as i64, y as i64, ctx.math);
                img.copy_from(&part, x as i64, y as i64);
                img
            }
            _ => {
                let mut img = RgbaImage::new(self.width, self.height, canvas);
                cache.draw(&units, &mut img, 0, 0, ctx.math);
                img
            }
        };
        cache.dirty = dirty;
        cache.previous = Some((base, sigs, out.clone()));
        cache.evict();
        out
    }
}

/// The units `el` draws, rendering (and caching) the ones not cached yet.
fn collect_units<'a>(el: &Element, ctx: &RenderCtx<'a>, cache: &mut RenderCache, units: &mut Vec<Unit<'a>>) {
    if !el.visible || el.opacity <= 0.0 {
        return;
    }
    let instance = matches!(el.kind, ElementKind::Instance { .. });
    let pass_through = el.is_pass_through();
    // A pass-through group's children are stable objects: cache each one. A pass-through
    // instance's children are resampled copies made on every render: cache it as a whole.
    if pass_through && !instance {
        for c in el.pass_through_children(ctx) {
            collect_units(&c, ctx, cache, units);
        }
        return;
    }
    let bounds = el.deep_bounds(ctx).filter(|b| b.0.is_finite() && b.1.is_finite());
    let Some((bx, by, _, _)) = bounds.filter(|_| pass_through || el.full_region(ctx).is_some()) else {
        units.push(Unit::Direct {
            el: Box::new(el.clone()),
            ctx: *ctx,
        });
        return;
    };
    // Whole pixels; the epsilon keeps float noise (6.9999999 after moving by 7) from moving the
    // anchor by a pixel.
    let (ax, ay) = ((bx + 1e-6).floor() as i64, (by + 1e-6).floor() as i64);
    // The object moved to its anchor: the same content anywhere has the same key.
    let mut local = el.clone();
    local.translate(-ax as f64, -ay as f64);
    let key = fingerprint(&local, ctx);
    let clock = cache.clock;
    units.push(Unit::Cached {
        key,
        ax,
        ay,
        opacity: el.opacity_byte(),
        blend: el.blend,
    });
    if let Some(e) = cache.entries.get_mut(&key) {
        e.used = clock;
        cache.last.0 += 1;
        return;
    }
    let mut pieces = Vec::new();
    if pass_through {
        let inner = ctx.deeper(&local);
        for c in local.pass_through_children(ctx) {
            collect_pieces(&c, &inner, &mut pieces);
        }
    } else if let Some((rx, ry, rw, rh)) = local.full_region(ctx) {
        // The same whole-object region as an uncached render.
        if let Some((img, x, y)) = local.pixels(rx, ry, rw, rh, ctx) {
            pieces.push(Piece {
                image: Arc::new(img),
                dx: rx + x,
                dy: ry + y,
                blend: None,
            });
        }
    }
    cache.last.1 += 1;
    let entry = Entry { pieces, used: clock };
    cache.bytes += entry.bytes();
    cache.entries.insert(key, entry);
}

/// The pixels a pass-through instance's child draws, as pieces (in the anchor-relative space
/// the child is in), entering nested pass-through objects as [`Element::render_onto`] does.
/// Children without a known size are skipped (instances are bounded by their symbol).
fn collect_pieces(el: &Element, ctx: &RenderCtx, out: &mut Vec<Piece>) {
    if !el.visible || el.opacity <= 0.0 {
        return;
    }
    if el.is_pass_through() {
        let inner = if matches!(el.kind, ElementKind::Instance { .. }) {
            ctx.deeper(el)
        } else {
            *ctx
        };
        for c in el.pass_through_children(ctx) {
            collect_pieces(&c, &inner, out);
        }
        return;
    }
    if let Some((rx, ry, rw, rh)) = el.full_region(ctx) {
        if let Some((img, x, y)) = el.pixels(rx, ry, rw, rh, ctx) {
            out.push(Piece {
                image: Arc::new(img),
                dx: rx + x,
                dy: ry + y,
                blend: Some((el.opacity_byte(), el.blend)),
            });
        }
    }
}

/// A hash of everything that affects an object's pixels (not its opacity or blend mode, which
/// apply when compositing).
/// A key for what a symbol's contents look like, independent of where its instance puts them.
pub(crate) fn contents_key(children: &[Element], ctx: &RenderCtx, detail: f64) -> u64 {
    let mut h = DefaultHasher::new();
    for c in children {
        hash_element(c, ctx, &mut h);
        c.opacity.to_bits().hash(&mut h);
        c.blend.fw_code().hash(&mut h);
    }
    (ctx.frame, ctx.depth, ctx.math as u8, detail.to_bits()).hash(&mut h);
    h.finish()
}

/// Rendered pixels and where they start.
#[derive(Clone)]
pub(crate) struct Kept {
    pub image: Arc<RgbaImage>,
    pub x: i64,
    pub y: i64,
}

/// The most recently used entries of a small memory, by key.
struct Memory {
    entries: Vec<(u64, Kept)>,
    limit: usize,
}

impl Memory {
    const fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            limit,
        }
    }

    fn get(&mut self, key: u64) -> Option<Kept> {
        let at = self.entries.iter().position(|(k, _)| *k == key)?;
        let entry = self.entries.remove(at);
        let kept = entry.1.clone();
        self.entries.push(entry);
        Some(kept)
    }

    fn put(&mut self, key: u64, kept: Kept) {
        const MAX_BYTES: usize = 256 << 20;
        self.entries.push((key, kept));
        let bytes = |e: &[(u64, Kept)]| {
            e.iter()
                .map(|(_, k)| k.image.width() as usize * k.image.height() as usize * 4)
                .sum::<usize>()
        };
        while self.entries.len() > 1 && (self.entries.len() > self.limit || bytes(&self.entries) > MAX_BYTES)
        {
            self.entries.remove(0);
        }
    }
}

thread_local! {
    static WARP_SOURCES: RefCell<Memory> = const { RefCell::new(Memory::new(6)) };
    static HIT_SHAPES: RefCell<Memory> = const { RefCell::new(Memory::new(6)) };
}

/// A perspective symbol's contents drawn flat, made once however often the pointer or a draw asks.
pub(crate) fn warp_source(key: u64, make: impl FnOnce() -> Option<Kept>) -> Option<Kept> {
    if let Some(kept) = WARP_SOURCES.with(|m| m.borrow_mut().get(key)) {
        return Some(kept);
    }
    let kept = make()?;
    WARP_SOURCES.with(|m| m.borrow_mut().put(key, kept.clone()));
    Some(kept)
}

/// Whether a perspective instance has pixels within `tol` of a point. Its shape is rendered once
/// and kept, as the Pointer asks on every mouse movement.
pub fn perspective_hit(el: &Element, px: i64, py: i64, tol: i64, ctx: &RenderCtx) -> bool {
    let key = fingerprint(el, ctx);
    let kept = HIT_SHAPES.with(|m| m.borrow_mut().get(key)).or_else(|| {
        let (rx, ry, rw, rh) = el.full_region(ctx)?;
        let (image, x, y) = el.pixels(rx, ry, rw, rh, ctx)?;
        let kept = Kept {
            image: Arc::new(image),
            x: rx + x,
            y: ry + y,
        };
        HIT_SHAPES.with(|m| m.borrow_mut().put(key, kept.clone()));
        Some(kept)
    });
    let Some(k) = kept else {
        return false;
    };
    (py - tol..=py + tol)
        .any(|y| (px - tol..=px + tol).any(|x| k.image.get(x - k.x, y - k.y).is_some_and(|p| p.a > 0)))
}

fn fingerprint(el: &Element, ctx: &RenderCtx) -> u64 {
    let mut h = DefaultHasher::new();
    hash_element(el, ctx, &mut h);
    ctx.frame.hash(&mut h);
    ctx.depth.hash(&mut h);
    (ctx.math as u8).hash(&mut h);
    h.finish()
}

/// `text` with every decimal number rounded to 1/10000 (a difference far below a visible one).
fn quantized(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut num = String::new();
    let flush = |num: &mut String, out: &mut String| {
        if num.contains(['.', 'e']) {
            match num.parse::<f64>() {
                Ok(v) => {
                    let r = (v * 10_000.0).round() / 10_000.0;
                    out.push_str(&format!("{}", if r == 0.0 { 0.0 } else { r }));
                }
                Err(_) => out.push_str(num),
            }
        } else {
            out.push_str(num);
        }
        num.clear();
    };
    for c in text.chars() {
        let part_of_number = c.is_ascii_digit()
            || c == '.'
            || (!num.is_empty() && (c == 'e' || ((c == '-' || c == '+') && num.ends_with('e'))))
            || (num.is_empty() && c == '-');
        if part_of_number {
            num.push(c);
        } else {
            flush(&mut num, &mut out);
            out.push(c);
        }
    }
    flush(&mut num, &mut out);
    out
}

fn hash_image(img: &RgbaImage, h: &mut DefaultHasher) {
    // Pixels are shared copy-on-write: the same buffer means the same pixels.
    (img.pixels().as_ptr() as usize, img.width(), img.height()).hash(h);
}

fn hash_element(el: &Element, ctx: &RenderCtx, h: &mut DefaultHasher) {
    el.visible.hash(h);
    for e in &el.effects {
        for (k, v) in &e.params {
            k.hash(h);
            match v {
                EffectParam::Text(t) => t.hash(h),
                EffectParam::Node(n) => format!("{n:?}").hash(h),
            }
        }
    }
    std::mem::discriminant(&el.kind).hash(h);
    match &el.kind {
        ElementKind::Bitmap(b) => {
            (b.x, b.y).hash(h);
            hash_image(&b.image, h);
        }
        ElementKind::Text(Text { cached, .. }) | ElementKind::Other { cached, .. } => {
            if let Some(b) = cached {
                (b.x, b.y).hash(h);
                hash_image(&b.image, h);
            }
        }
        // Paths are small: their full description, with coordinates rounded so float noise
        // from moving there and back doesn't change the key.
        ElementKind::Path(p) => quantized(&format!("{p:?}")).hash(h),
        ElementKind::Group(children) => {
            for c in children {
                hash_element(c, ctx, h);
                c.opacity.to_bits().hash(h);
                c.blend.fw_code().hash(h);
            }
        }
        ElementKind::Masked {
            content,
            mask,
            mode,
            cache,
        } => {
            for c in content {
                hash_element(c, ctx, h);
                c.opacity.to_bits().hash(h);
                c.blend.fw_code().hash(h);
            }
            hash_element(mask, ctx, h);
            (mask.opacity.to_bits(), mask.blend.fw_code()).hash(h);
            format!("{mode:?}").hash(h);
            if let Some(c) = cache {
                hash_element(c, ctx, h);
            }
        }
        ElementKind::Instance { symbol, matrix, .. } => {
            symbol.hash(h);
            (ctx.mx_perspective && !matrix.is_affine()).hash(h);
            let m = [
                matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f, matrix.g, matrix.h, matrix.i,
            ];
            quantized(&format!("{m:?}")).hash(h);
            // The symbol's contents (what the instance shows in this frame).
            if let Some(children) = el.symbol_elements(ctx) {
                let inner = ctx.deeper(el);
                for c in &children {
                    hash_element(c, &inner, h);
                    c.opacity.to_bits().hash(h);
                    c.blend.fw_code().hash(h);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Bitmap, ElementKind};
    use firecraft_raster::Rgba;

    #[test]
    fn quantizes_float_noise() {
        assert_eq!(
            quantized("Pt { x: 0.30000000000000004, y: -2.0e-17, n: 3 }"),
            quantized("Pt { x: 0.3, y: 0.0, n: 3 }")
        );
        assert_ne!(quantized("x: 0.3"), quantized("x: 0.31"));
    }

    #[test]
    fn moving_an_object_reuses_its_pixels() {
        let mut doc = Document::new(100, 100, 72.0, Some(Rgba::WHITE));
        let img = RgbaImage::new(20, 20, Rgba::new(255, 0, 0, 255));
        let mut el = Element::new(ElementKind::Bitmap(Bitmap {
            x: 10,
            y: 10,
            image: img,
        }));
        el.effects.push(crate::EffectEntry::from_pairs(&[
            ("EffectIsVisible", "true"),
            ("EffectMoaID", "{d04ef8c0-71b3-11d1-8c8200a024cdc039}"),
            ("gaussian_blur_radius", "3"),
        ]));
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v.push(el);
        }
        let mut cache = RenderCache::new();
        let first = doc.render_cached(0, &mut cache);
        assert_eq!(cache.last, (0, 1));
        assert_eq!(first, doc.render(0));
        // Moved: a cache hit, and the same picture as an uncached render.
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v[0].translate(30.0, 25.0);
        }
        let moved = doc.render_cached(0, &mut cache);
        assert_eq!(cache.last, (1, 0));
        assert_eq!(moved, doc.render(0));
        // Only the area it left and the area it moved to were composited again.
        assert!(matches!(cache.dirty, Dirty::Rect { .. }), "{:?}", cache.dirty);
        // Nothing changed: nothing to redo.
        assert_eq!(doc.render_cached(0, &mut cache), moved);
        assert_eq!(cache.dirty, Dirty::None);
    }
}
