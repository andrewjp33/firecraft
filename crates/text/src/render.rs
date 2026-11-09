//! Rasterung eines [`TextLayout`]: Deckungsmaske, farbiges Bild und Glyphenkonturen
//! („In Pfade konvertieren“, 09 §5.8).
//!
//! Ablauf (09 §6.1): Glyphenkonturen an ihre Lage abbilden, Faux-Fett als verbreiterte Polygone,
//! Unterstreichungen als Rechtecke, alles zusammen mit Punktabtastung N×N zählen (Non-Zero, also
//! Vereinigung), Trefferzahl über die Tonkurve auf Alpha. Die Maske dient fp-render als Deckung für
//! die Füllung des Text-`PATv` (Vollfarbe, Verlauf, Muster); [`render`] liefert direkt das Bild mit
//! den Laufarben (`FCLi`).

use crate::support::Rgba8;
use crate::support::{BezierContour, BezierNode, IRect, Point};
use crate::support::{Mask, RgbaImage};

use crate::antialias;
use crate::font::face::HintStyle;
use crate::font::outline::to_bezier_contour;
use crate::layout::{FAUX_BOLD_STROKE, PositionedGlyph, TextLayout};
use crate::model::AntiAliasMode;
use crate::raster::{
    Affine, Edge, RASTER_TOLERANCE, contours_to_polygons, embolden, push_contour, push_polygon, sample_counts,
};

/// Pixelgrenzen der Tinte (Glyphen inkl. Faux-Fett und Unterstreichungen), nach außen gerundet.
/// Leeres Rechteck, wenn nichts sichtbar ist.
pub fn ink_bounds(layout: &TextLayout) -> IRect {
    let mut r: Option<[f64; 4]> = None;
    let mut add = |x0: f64, y0: f64, x1: f64, y1: f64| {
        r = Some(match r {
            None => [x0, y0, x1, y1],
            Some(o) => [o[0].min(x0), o[1].min(y0), o[2].max(x1), o[3].max(y1)],
        });
    };
    for g in layout.visible_glyphs() {
        if let Some(b) = g.ink_bounds() {
            add(b.left as f64, b.top as f64, b.right as f64, b.bottom as f64);
        }
    }
    for d in &layout.decorations {
        for p in d.quad {
            add(p[0], p[1], p[0], p[1]);
        }
    }
    match r {
        Some(b) if b.iter().all(|v| v.is_finite()) => {
            let c = |v: f64| v.clamp(-1.0e9, 1.0e9);
            IRect::new(
                c(b[0]).floor() as i32,
                c(b[1]).floor() as i32,
                c(b[2]).ceil() as i32,
                c(b[3]).ceil() as i32,
            )
        }
        _ => IRect::new(0, 0, 0, 0),
    }
}

/// Hinting-Art der Rasterung für ein Layout (09 §6.4, FX-T): volles Hinting („Mono“) in der
/// N-fachen Auflösung der Überabtastung, außer bei „Stark“ (eigener Pfad im Original, ungehintet
/// näher) und bei abgeschaltetem Hinting ([`crate::LayoutOptions::hinting`]).
fn hint_style(layout: &TextLayout) -> Option<HintStyle> {
    let strong = layout.antialias.enabled && layout.antialias.mode.effective() == AntiAliasMode::Strong;
    (layout.hinting && !strong).then_some(HintStyle::Mono)
}

/// Lage einer gehinteten Glyphe wie in der Schriftmaschine des Originals: Grundlinie auf ganze
/// Pixel gerundet, waagerechter Ursprung auf das Raster der Überabtastung (1/N px).
fn snapped_origin(t: &Affine, nf: f64) -> (f64, f64) {
    ((t.e * nf).round() / nf, t.f.round())
}

/// Ist die Abbildung eine reine, gleichmäßige Skalierung mit Verschiebung (ohne Drehung, Scherung,
/// Spiegelung, Breitenskalierung)? Nur dann wird gehintet.
fn axis_aligned(t: &Affine) -> bool {
    t.b == 0.0 && t.c == 0.0 && t.a > 0.0 && (t.a + t.d).abs() <= 1e-9 * t.a.abs().max(1.0)
}

/// Strecken einer Glyphe relativ zum Ursprung (ox, oy); `n` = Überabtastung.
fn glyph_edges(
    g: &PositionedGlyph,
    ox: f64,
    oy: f64,
    n: u32,
    hint: Option<HintStyle>,
    edges: &mut Vec<Edge>,
) {
    if !g.visible || g.is_tab {
        return;
    }
    let outline = g.font.outline(g.glyph_id);
    if outline.is_empty() {
        return;
    }
    let t = g.transform;
    if let Some(style) = hint
        && axis_aligned(&t)
    {
        let nf = n.max(1) as f64;
        let ppem = t.a * g.font.units_per_em() as f64 * nf;
        if let Some(h) = g.font.hinted_outline(g.glyph_id, ppem as f32, style) {
            let (ex, fy) = snapped_origin(&t, nf);
            let m = Affine {
                a: 1.0 / nf,
                b: 0.0,
                c: 0.0,
                d: -1.0 / nf,
                e: ex - ox,
                f: fy - oy,
            };
            if g.faux_bold {
                let mut polys = contours_to_polygons(&h.contours, &m, RASTER_TOLERANCE);
                embolden(&mut polys, g.size * FAUX_BOLD_STROKE);
                for p in &polys {
                    push_polygon(edges, p);
                }
            } else {
                for c in &h.contours {
                    push_contour(edges, c, &m, RASTER_TOLERANCE);
                }
            }
            return;
        }
    }
    let m = g.transform.translated(-ox, -oy);
    if g.faux_bold {
        let mut polys = contours_to_polygons(&outline.contours, &m, RASTER_TOLERANCE);
        embolden(&mut polys, g.size * FAUX_BOLD_STROKE);
        for p in &polys {
            push_polygon(edges, p);
        }
    } else {
        for c in &outline.contours {
            push_contour(edges, c, &m, RASTER_TOLERANCE);
        }
    }
}

/// Trefferzahlen je Pixel (0…N²) und N für die Glyphen, die `filter` zulässt.
fn counts_for(
    layout: &TextLayout,
    bounds: IRect,
    n: u32,
    filter: &dyn Fn(&PositionedGlyph) -> bool,
    deco: Option<Rgba8>,
) -> Vec<u16> {
    let (ox, oy) = (bounds.left as f64, bounds.top as f64);
    let mut edges = Vec::new();
    let hint = hint_style(layout);
    for g in layout.glyphs.iter().filter(|g| filter(g)) {
        glyph_edges(g, ox, oy, n, hint, &mut edges);
    }
    for d in &layout.decorations {
        if deco.is_none_or(|c| c == d.color) {
            let q: Vec<[f64; 2]> = d.quad.iter().map(|p| [p[0] - ox, p[1] - oy]).collect();
            push_polygon(&mut edges, &q);
        }
    }
    sample_counts(&edges, bounds.width() as usize, bounds.height() as usize, n)
}

fn checked(bounds: IRect) -> Option<IRect> {
    let (w, h) = (bounds.width() as u64, bounds.height() as u64);
    if w == 0 || h == 0 || w * h > (1 << 26) {
        None
    } else {
        Some(bounds)
    }
}

/// Rasterbereich ohne Vorgabe: [`ink_bounds`] plus Sicherheitsrand, weil Hinting und die
/// gerundete Grundlinie die Tinte um Bruchteile eines Pixels verschieben.
fn raster_bounds(layout: &TextLayout) -> IRect {
    let b = ink_bounds(layout);
    if b.is_empty() { b } else { b.inflate(2, 2) }
}

/// Deckungsmaske (Alpha nach Tonkurve) in `bounds`; ohne `bounds` wie [`render`]: Rechteck der
/// gedeckten Pixel plus 1 px Rand.
pub fn coverage(layout: &TextLayout, bounds: Option<IRect>) -> Mask {
    if let Some(b) = bounds {
        return coverage_in(layout, b);
    }
    let m = coverage_in(layout, raster_bounds(layout));
    let (b, w) = (m.bounds(), m.width());
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (i, &v) in m.data().iter().enumerate() {
        if v != 0 {
            let (x, y) = (b.left + (i % w) as i32, b.top + (i / w) as i32);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 > x1 {
        let ib = ink_bounds(layout);
        return Mask::empty_at(ib.left, ib.top);
    }
    m.copy_rect(IRect::new(x0, y0, x1, y1).inflate(1, 1)).unwrap_or(m)
}

fn coverage_in(layout: &TextLayout, b: IRect) -> Mask {
    let Some(b) = checked(b) else {
        return Mask::empty_at(b.left, b.top);
    };
    let (n, lut) = antialias::sampling(&layout.antialias);
    let counts = counts_for(layout, b, n, &|_| true, None);
    let data: Vec<u8> = counts
        .iter()
        .map(|&c| lut[(c as usize).min(lut.len() - 1)])
        .collect();
    Mask::from_raw(b, data).unwrap_or_else(|_| Mask::empty_at(b.left, b.top))
}

/// Run colours of the visible ink (glyphs first, then decorations) in first-seen order: the
/// colours [`render`] mixes per pixel.
pub fn ink_colors(layout: &TextLayout) -> Vec<Rgba8> {
    let mut colors: Vec<Rgba8> = Vec::new();
    for g in layout.visible_glyphs() {
        if !colors.contains(&g.color) {
            colors.push(g.color);
        }
    }
    for d in &layout.decorations {
        if !colors.contains(&d.color) {
            colors.push(d.color);
        }
    }
    colors
}

/// Coverage (alpha after the tone curve) in `bounds` of the ink drawn in `color` only: glyphs of
/// that run colour and decorations of that colour. Used by renderers that post-process the fill
/// edge (feather, dither) and need the colour weights of every pixel.
pub fn color_coverage(layout: &TextLayout, bounds: IRect, color: Rgba8) -> Mask {
    let Some(b) = checked(bounds) else {
        return Mask::empty_at(bounds.left, bounds.top);
    };
    let (n, lut) = antialias::sampling(&layout.antialias);
    let counts = counts_for(layout, b, n, &|g| g.color == color, Some(color));
    let data: Vec<u8> = counts
        .iter()
        .map(|&c| lut[(c as usize).min(lut.len() - 1)])
        .collect();
    Mask::from_raw(b, data).unwrap_or_else(|_| Mask::empty_at(b.left, b.top))
}

/// Trefferzahlen (0…N²) und N der Glättungseinstellung, z. B. für Vergleiche mit Render-Caches.
pub fn sample_layout(layout: &TextLayout, bounds: IRect) -> (Vec<u16>, u32) {
    let (n, _) = antialias::sampling(&layout.antialias);
    (sample_layout_n(layout, bounds, n), n)
}

/// Trefferzahlen (0…n²) je Pixel in `bounds` mit frei gewählter Überabtastung `n`.
pub fn sample_layout_n(layout: &TextLayout, bounds: IRect, n: u32) -> Vec<u16> {
    match checked(bounds) {
        Some(b) => counts_for(layout, b, n, &|_| true, None),
        None => Vec::new(),
    }
}

/// Rendert den Text mit den Laufarben in ein Bild.
///
/// Mit `bounds` genau in diesem Rechteck. Ohne `bounds` wie das Objektbild des Originals: das
/// Rechteck der sichtbaren Pixel **plus 1 px transparenter Rand** ringsum (FX-1/FX-T: Effekte wie
/// der Massive Schatten und die Weichzeichner rechnen auf diesem Bild; ohne den Rand fehlt eine
/// Zeile/Spalte).
///
/// Alpha = Tonkurve(Treffer der Vereinigung aller Glyphen) · Farbalpha. Bei mehreren Farben
/// mittelt ein Pixel die Farben nach ihren Treffern (wie die Verkleinerungsroutine, die die
/// Farbkanäle der gesetzten Unterpixel mittelt, 09 §6.1).
pub fn render(layout: &TextLayout, bounds: Option<IRect>) -> RgbaImage {
    if let Some(b) = bounds {
        return render_in(layout, b);
    }
    let img = render_in(layout, raster_bounds(layout));
    let ab = img.alpha_bounds();
    if ab.is_empty() {
        let b = ink_bounds(layout);
        return RgbaImage::empty_at(b.left, b.top);
    }
    img.copy_rect(ab.inflate(1, 1)).unwrap_or(img)
}

fn render_in(layout: &TextLayout, b: IRect) -> RgbaImage {
    let Some(b) = checked(b) else {
        return RgbaImage::empty_at(b.left, b.top);
    };
    let (n, lut) = antialias::sampling(&layout.antialias);
    let colors = ink_colors(layout);
    let total = counts_for(layout, b, n, &|_| true, None);
    let per: Vec<Vec<u16>> = if colors.len() > 1 {
        colors
            .iter()
            .map(|c| counts_for(layout, b, n, &|g| g.color == *c, Some(*c)))
            .collect()
    } else {
        Vec::new()
    };
    let mut data = vec![[0u8; 4]; total.len()];
    for (i, px) in data.iter_mut().enumerate() {
        let t = total[i] as usize;
        if t == 0 {
            continue;
        }
        let alpha = lut[t.min(lut.len() - 1)] as u32;
        let c = if colors.len() <= 1 {
            colors.first().copied().unwrap_or(Rgba8::new(0, 0, 0, 255))
        } else {
            let (mut r, mut g, mut bb, mut a, mut s) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for (ci, c) in colors.iter().enumerate() {
                let k = per[ci][i] as u32;
                r += c.r as u32 * k;
                g += c.g as u32 * k;
                bb += c.b as u32 * k;
                a += c.a as u32 * k;
                s += k;
            }
            let avg = |v: u32| (v + s / 2).checked_div(s).map(|q| q as u8);
            match (avg(r), avg(g), avg(bb), avg(a)) {
                (Some(r), Some(g), Some(bb), Some(a)) => Rgba8::new(r, g, bb, a),
                _ => colors[0],
            }
        };
        let a = (alpha * c.a as u32 + 127) / 255;
        *px = [c.r, c.g, c.b, a as u8];
    }
    RgbaImage::from_raw(b, data).unwrap_or_else(|_| RgbaImage::empty_at(b.left, b.top))
}

/// Konturen aller sichtbaren Glyphen in Dokumentkoordinaten (je Glyphe eine Liste), für
/// „In Pfade konvertieren“ und fp-vector. Faux-Fett liefert die verbreiterten Polygone als
/// Eckpunkt-Konturen, Unterstreichungen erscheinen als eigene Rechteck-Konturen am Ende.
pub fn glyph_contours(layout: &TextLayout) -> Vec<Vec<BezierContour>> {
    let mut out = Vec::new();
    for g in layout.visible_glyphs() {
        if g.is_tab {
            continue;
        }
        let o = g.font.outline(g.glyph_id);
        if o.is_empty() {
            continue;
        }
        let m = g.transform;
        if g.faux_bold {
            let mut polys = contours_to_polygons(&o.contours, &m, RASTER_TOLERANCE);
            embolden(&mut polys, g.size * FAUX_BOLD_STROKE);
            out.push(
                polys
                    .iter()
                    .map(|p| {
                        BezierContour::new(
                            p.iter()
                                .map(|q| BezierNode::corner(Point::new(q[0] as f32, q[1] as f32)))
                                .collect(),
                            true,
                        )
                    })
                    .collect(),
            );
        } else {
            out.push(
                o.contours
                    .iter()
                    .map(|c| to_bezier_contour(&c.iter().map(|s| s.map(|p| m.apply(p))).collect()))
                    .collect(),
            );
        }
    }
    for d in &layout.decorations {
        out.push(vec![BezierContour::new(
            d.quad
                .iter()
                .map(|q| BezierNode::corner(Point::new(q[0] as f32, q[1] as f32)))
                .collect(),
            true,
        )]);
    }
    out
}

/// Wie [`glyph_contours`], aber jede Glyphe an das Pixelraster angeglichen
/// ([`crate::outline_hint`], TW-56b): Ergebnis von „Text ▸ In Pfade konvertieren“. Die
/// Textdarstellung selbst bleibt ungehintet.
pub fn hinted_glyph_contours(layout: &TextLayout) -> Vec<Vec<BezierContour>> {
    let mut out = glyph_contours(layout);
    for g in &mut out {
        crate::outline_hint::hint_glyph(g);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_layout_renders_empty() {
        let lay = crate::layout::layout(
            &crate::model::TextBlock::default(),
            &crate::font::FontDb::new(),
            &crate::layout::LayoutOptions::default(),
        );
        assert!(ink_bounds(&lay).is_empty());
        assert!(render(&lay, None).is_empty());
        assert_eq!(coverage(&lay, None).width(), 0);
        assert!(glyph_contours(&lay).is_empty());
    }
}
