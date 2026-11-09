//! Strich eines Pfads: Stempel-Engine (Spezifikation 07 §6, Referenz `re\vektor\stroke.py`).
//!
//! Ablauf (`@0084e290` → `@0078a7b0` → `@007852f0`):
//! 1. **Wirksame Platzierung** ([`effective_placement`], `@0084df60`): eine offene Kontur → mittig;
//!    alle geschlossen, `BRPi = 1`, `PTTi = 1` und `BDIi ≤ 1` → innen; sonst `BRPi`.
//! 2. **Wirksamer Durchmesser** ([`effective_diameter`]): mittig `D`, innen/außen `2D`, beim
//!    simple-Pinsel ohne AA innen `2D − 1`, außen `2D + 1`.
//! 3. **Läufer**: simple ([`crate::brush::walk_simple`]), natural ([`crate::natural`]), oder
//!    Walker-Modus 2 (simple ohne AA, innen/außen, `@788350`): Pfad hart rastern und auf jedes
//!    gesetzte Pixel mit ungesetztem 4-Nachbarn stempeln.
//! 4. **Stempeln** in einen 8-Bit-Strichpuffer: `sa = mul(Maske, O)`,
//!    `a′ = da + sa − mul(da, sa)`, bei `BFBi = 0` gedeckelt bei `O`. Deckkraft `O` über die
//!    Überlappungstabelle und den Promille-Umweg ([`crate::support::color::brush_stamp_opacity`]).
//!    `BFBi = 2` zeichnet nichts.
//! 5. **Puffergrenzen** (`@0078ab20`, T5): Vereinigung der Stempelrechtecke
//!    `[(p16 − h)/16, (p16 − h)/16 + w)`, auf ganze Pixel erweitert; darüber hinaus wird abgeschnitten.
//! 6. **Innen/außen-Maske**: innen × Füllmaske des Pfads, außen × (255 − Füllmaske); Kantenglättung
//!    der Maske = Pinsel-AA (0 nur bei simple ohne AA), nicht `FETi`.
//! 7. **Farbe**: Strichfarbe `BCLi`, Puffer-Alpha × Farb-Alpha.

use crate::attrs::{BrushType, Feedback, Stroke, StrokePlacement, VectorPath};
use crate::brush::{Dash, SimpleStep, Tip, shift_tip, tip_mask, walk_simple_steps};
use crate::coverage::{Coverage, fill_coverage};
use crate::paint::mul;
use crate::support::geom::IRect;

/// Wirksame Strichposition (`@0084df60`, B; `PTTi`-Zuordnung A, 8/8 Fälle).
pub fn effective_placement(path: &VectorPath, stroke: &Stroke) -> StrokePlacement {
    if !path.contours.iter().all(|c| c.closed) {
        return StrokePlacement::Center;
    }
    if path.placement == StrokePlacement::Center && path.ptt && stroke.brush.diameter <= 1 {
        return StrokePlacement::Inside;
    }
    path.placement
}

/// Wirksamer Durchmesser `D′` (`@785470`, `@7853e0`, `@786010`).
pub fn effective_diameter(stroke: &Stroke, placement: StrokePlacement) -> f64 {
    let d = stroke.brush.diameter as f64;
    let b = &stroke.brush;
    match placement {
        StrokePlacement::Center => d,
        _ if !b.antialiased && b.kind == BrushType::Simple => {
            if placement == StrokePlacement::Inside {
                2.0 * d - 1.0
            } else {
                2.0 * d + 1.0
            }
        }
        _ => 2.0 * d,
    }
}

/// Ein Stempel-Ereignis: Position in 1/16 px (vor dem Abzug der halben Spitze), Deckkraft, Spitze.
#[derive(Debug, Clone, Copy)]
pub struct Dab {
    pub x16: i64,
    pub y16: i64,
    /// Deckkraft-Byte des Stempels.
    pub opacity: u8,
    /// Index der Spitze in der Spitzenliste (Größen-/Winkelstufe).
    pub tip: usize,
}

/// 8-Bit-Strichpuffer über einem Fenster mit Grenzen der Stempelrechtecke.
pub(crate) struct InkBuffer<'a> {
    pub rect: IRect,
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
    /// Vereinigung der Stempelrechtecke (links, oben, rechts, unten) in Dokumentpixeln.
    pub bounds: Option<(i64, i64, i64, i64)>,
    /// Strichtextur (Nachschlag, `BTBi`, `BTEi`), wirkt je Stempelpixel auf den Maskenwert.
    pub tex: Option<(crate::natural::TexLookup<'a>, u32, u32)>,
    /// Per row, a run of pixels known to be at full ink: a further stamp can't change them, so a
    /// large tip only pays for its rim instead of re-reading all it already covered.
    full: Vec<FullRun>,
}

/// Columns `start..end` of a window row, all holding `value`.
#[derive(Clone, Copy, Debug, Default)]
struct FullRun {
    start: i64,
    end: i64,
    value: u8,
}

impl InkBuffer<'_> {
    pub fn new(rect: IRect) -> Self {
        let (w, h) = (rect.width() as usize, rect.height() as usize);
        Self {
            rect,
            w,
            h,
            data: vec![0; w * h],
            bounds: None,
            tex: None,
            full: Vec::new(),
        }
    }

    /// Stempelrechteck `[(p16)/16, p16/16 + tipw)` auf ganze Pixel erweitert vormerken.
    pub fn note_bounds(&mut self, px16: i64, py16: i64, tipw: f64) {
        let fx0 = px16 as f64 / 16.0;
        let fy0 = py16 as f64 / 16.0;
        let b = (
            fx0.floor() as i64,
            fy0.floor() as i64,
            (fx0 + tipw).ceil() as i64,
            (fy0 + tipw).ceil() as i64,
        );
        self.bounds = Some(match self.bounds {
            None => b,
            Some(o) => (o.0.min(b.0), o.1.min(b.1), o.2.max(b.2), o.3.max(b.3)),
        });
    }

    /// Maske an `(x, y)` (Dokument) mit Deckkraft `o` stempeln.
    #[inline]
    pub fn stamp(&mut self, x: i64, y: i64, m: &Tip, o: u8, cap: bool) {
        let (x0, y0) = (self.rect.left as i64, self.rect.top as i64);
        // Stempel ganz außerhalb des Fensters: nichts zu tun (Teilfenster der Live-Vorschau)
        if x >= x0 + self.w as i64 || y >= y0 + self.h as i64 || x + m.n as i64 <= x0 || y + m.n as i64 <= y0
        {
            return;
        }
        // Only the part of the tip inside the window.
        let n = m.n as i64;
        let (j0, j1) = ((y0 - y).max(0), (y0 + self.h as i64 - y).min(n));
        let (i0, i1) = ((x0 - x).max(0), (x0 + self.w as i64 - x).min(n));
        if self.full.len() != self.h {
            self.full = vec![FullRun::default(); self.h];
        }
        // What a pixel can't rise above: it stays there whatever is stamped over it.
        let full = if cap { o } else { 255 };
        let off = x - x0;
        for j in j0 as usize..j1 as usize {
            let yy = (y + j as i64 - y0) as usize;
            let tip_row = m.rows[j];
            let (a, b) = (i64::from(tip_row.first).max(i0), i64::from(tip_row.end).min(i1));
            if a >= b {
                continue;
            }
            let known = self.full[yy];
            let usable = known.start < known.end && known.value == full;
            let (mut skip0, mut skip1) = (b, b);
            if usable {
                let (s0, s1) = ((known.start - off).max(a), (known.end - off).min(b));
                if s0 < s1 {
                    (skip0, skip1) = (s0, s1);
                }
            }
            let left = self.stamp_columns(m, j, yy, off, a..skip0, o, cap, full);
            let right = self.stamp_columns(m, j, yy, off, skip1..b, o, cap, full);
            // A cap at another level changes pixels the run promised were settled.
            if !usable && known.start < b + off && a + off < known.end {
                self.full[yy] = FullRun::default();
            }
            for (start, end) in [left, right] {
                if start >= end {
                    continue;
                }
                let slot = &mut self.full[yy];
                let new = FullRun {
                    start,
                    end,
                    value: full,
                };
                if slot.start >= slot.end || slot.value != full {
                    *slot = new;
                } else if start <= slot.end && slot.start <= end {
                    slot.start = slot.start.min(start);
                    slot.end = slot.end.max(end);
                } else if end - start > slot.end - slot.start {
                    *slot = new;
                }
            }
        }
    }

    /// Stamp tip row `j`'s columns `cols` onto window row `yy`: the longest run of window columns
    /// left at `full`, which no later stamp can change.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn stamp_columns(
        &mut self,
        m: &Tip,
        j: usize,
        yy: usize,
        off: i64,
        cols: std::ops::Range<i64>,
        o: u8,
        cap: bool,
        full: u8,
    ) -> (i64, i64) {
        // Written for unoptimised builds too: plain slices and a while loop, with the blend
        // spelled out, as a function call per pixel is most of a debug build's cost here.
        let (x0, y0) = (self.rect.left as i64, self.rect.top as i64);
        let row = yy * self.w;
        let (mut best, mut run) = ((0i64, 0i64), (0i64, 0i64));
        let tip: &[u8] = &m.data[j * m.n..(j + 1) * m.n];
        let line: &mut [u8] = &mut self.data[row..row + self.w];
        let tex = self.tex.as_ref();
        let opacity = u32::from(o);
        let limit = if cap { opacity } else { 255 };
        let (mut i, end) = (cols.start, cols.end);
        while i < end {
            let column = i;
            i += 1;
            let mut v = tip[column as usize];
            if v == 0 {
                continue;
            }
            let xx = column + off;
            if let Some((t, btb, bte)) = tex {
                v = crate::natural::textured_mask(
                    v,
                    t.value((x0 + xx) as i32, (y0 + yy as i64) as i32),
                    *btb,
                    *bte,
                );
            }
            // mul255(v, o), then the source-over of `sa` on `da`.
            let t = u32::from(v) * opacity + 128;
            let sa = (t + (t >> 8)) >> 8;
            let k = xx as usize;
            let da = u32::from(line[k]);
            let t = da * sa + 128;
            let a2 = da + sa - ((t + (t >> 8)) >> 8);
            let value = if a2 < limit { a2 } else { limit } as u8;
            line[k] = value;
            if value == full {
                if run.1 == xx && run.1 > run.0 {
                    run.1 += 1;
                } else {
                    run = (xx, xx + 1);
                }
                if run.1 - run.0 > best.1 - best.0 {
                    best = run;
                }
            }
        }
        best
    }

    /// Alles außerhalb der Stempelrechtecke löschen (`STAMP_BOUNDS`, T5).
    pub fn clip_to_bounds(&mut self) {
        let Some((bx0, by0, bx1, by1)) = self.bounds else {
            return;
        };
        let (x0, y0) = (self.rect.left as i64, self.rect.top as i64);
        for j in 0..self.h {
            let yy = y0 + j as i64;
            let row_out = yy < by0 || yy >= by1;
            for i in 0..self.w {
                let xx = x0 + i as i64;
                if row_out || xx < bx0 || xx >= bx1 {
                    self.data[j * self.w + i] = 0;
                }
            }
        }
    }
}

/// Strichebene als RGBA über `rect` (Referenz `stroke_layer`). Natürliche Pinsel laufen über
/// [`crate::natural`]; `None` bedeutet „keine Pixel“.
pub fn stroke_layer(path: &VectorPath, stroke: &Stroke, rect: IRect) -> Vec<[u8; 4]> {
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    let mut out = vec![[0u8; 4]; w * h];
    let b = &stroke.brush;
    if b.feedback == Feedback::Background || w == 0 || h == 0 {
        return out;
    }
    let placement = effective_placement(path, stroke);
    let natural = b.kind == BrushType::Natural;
    let ink = if natural {
        crate::natural::natural_ink(path, stroke, placement, rect)
    } else {
        simple_ink(path, stroke, placement, rect)
    };
    let mut buf = ink.data;
    if placement != StrokePlacement::Center {
        let polys = path.polys();
        let fc = fill_coverage(&polys, rect, path.even_odd, path.agm, natural || b.antialiased);
        for (v, c) in buf.iter_mut().zip(&fc.data) {
            if *v != 0 {
                let m = if placement == StrokePlacement::Inside {
                    *c
                } else {
                    255 - *c
                };
                *v = mul(*v, m);
            }
        }
    }
    if b.alpha_remap != 0 {
        // alphaRemap im Endcomposite (`@006e96e0`, Flag 0x1c), nach der Innen/außen-Maske
        // (Laufzeit T2 3D Glow/Toxic Waste: Kreis innen)
        let table = crate::natural::alpha_remap_table(b.alpha_remap, b.concentration);
        for v in buf.iter_mut() {
            *v = table[*v as usize];
        }
    }
    let col = stroke.color;
    let ca = col.a();
    for (k, (o, &a)) in out.iter_mut().zip(&buf).enumerate() {
        if a != 0 {
            let a = if ca != 255 { mul(a, ca) } else { a };
            let rgb = match &ink.rgb {
                Some(v) => v[k],
                None => [col.r(), col.g(), col.b()],
            };
            *o = [rgb[0], rgb[1], rgb[2], a];
        }
    }
    out
}

/// Strichpuffer der simple-Pinsel (`@7876b0`, `@788350`, `@788ae0`).
///
/// re 39 §2.2: Der simple-Läufer interpoliert Druck und Geschwindigkeit je Stempel (16.16) und ruft
/// dieselbe Stempel-Funktion wie der natürliche: Größen-/Winkelstufe ([`crate::natural`] `stage`)
/// mit der Spitzenmatrix je Stufe und die Deckkraft-Empfindlichkeit. Abstand und Fluss liest er
/// nicht.
pub(crate) fn simple_ink(
    path: &VectorPath,
    stroke: &Stroke,
    placement: StrokePlacement,
    rect: IRect,
) -> InkBufferOut {
    let mut ink = InkBuffer::new(rect);
    let mut rgb_ink =
        crate::natural::simple_needs_rgb(&stroke.brush).then(|| crate::natural::ArgbInk::new(rect));
    simple_run(path, stroke, placement, rect, &mut ink, &mut rgb_ink, None);
    if let Some(r) = rgb_ink {
        return r.into_out();
    }
    ink.clip_to_bounds();
    InkBufferOut {
        data: ink.data,
        rgb: None,
    }
}

/// What a simple brush's walk carries from one piece of a live stroke to the next.
#[derive(Clone, Debug, Default)]
pub(crate) struct SimpleResume {
    acc: Option<i64>,
    dash: Option<Dash>,
    dash_ready: bool,
}

/// Stamp `path` into the buffers. With `resume`, `path` is the next piece of one open contour.
fn simple_run<'a>(
    path: &VectorPath,
    stroke: &'a Stroke,
    placement: StrokePlacement,
    rect: IRect,
    ink: &mut InkBuffer<'a>,
    rgb_ink: &mut Option<crate::natural::ArgbInk<'a>>,
    resume: Option<&mut SimpleResume>,
) {
    use crate::attrs::{SenseTarget as Tgt, TipShape};
    use crate::brush::{Minstd, TipTransform, transformed_tip};
    use crate::natural::{PenState, sensitivity_speed, simple_stamp};
    let b = &stroke.brush;
    let aa = b.antialiased;
    let dp = effective_diameter(stroke, placement);
    let round = b.shape == TipShape::Circle;
    let tip_key = crate::tipcache::TipKey {
        natural: false,
        dp,
        round,
        soft: b.softness,
        bell: b.bell_curve,
        aspect: b.aspect,
        angle: b.angle,
        min_size: b.min_size,
        m: b.max_count.min(64) as usize,
        angle_mode: b.row_active(Tgt::Angle),
    };
    let crate::tipcache::Tips {
        mut stages,
        shifted: mut cache,
    } = crate::tipcache::take(tip_key);
    let tip = stages
        .entry(0)
        .or_insert_with(|| tip_mask(dp, round, b.softness, b.bell_curve))
        .clone();
    let n_tip = tip.n as i64 - 1;
    let o_base =
        crate::support::color::brush_stamp_opacity(b.concentration, feedback_code(b.feedback), b.spacing, dp);
    let cap = b.feedback != Feedback::Brush;
    let tex = stroke
        .texture
        .as_ref()
        .filter(|_| b.texture_blend > 0 || b.texture_edge > 0)
        .map(|t| crate::natural::TexLookup::new(&t.mask, path.texture_offset));
    ink.tex = tex.map(|t| (t, b.texture_blend, b.texture_edge));
    let tex_rgb = stroke
        .texture
        .as_ref()
        .filter(|_| rgb_ink.is_some() && (b.texture_blend > 0 || b.texture_edge > 0))
        .map(|t| crate::natural::TexLookup::new(&t.mask, path.texture_offset));
    let m = b.max_count.min(64);
    let angle_mode = b.row_active(Tgt::Angle);
    let staged = m > 0 && (angle_mode || b.row_active(Tgt::Size));
    let color = [stroke.color.r(), stroke.color.g(), stroke.color.b()];
    let mut tip_for = |k: usize| -> Tip {
        if k == 0 {
            return tip.clone();
        }
        stages
            .entry(k)
            .or_insert_with(|| {
                let t = TipTransform {
                    aspect_permille: b.aspect,
                    angle_deg: b.angle,
                    size_stage: (!angle_mode).then_some((k as u32, m, b.min_size)),
                    angle_stage: angle_mode.then_some((k as u32, m)),
                };
                transformed_tip(dp, round, b.softness, b.bell_curve, &t)
            })
            .clone()
    };
    let walks = match (resume, path.contours.first()) {
        (Some(r), Some(c)) => {
            if !r.dash_ready {
                r.dash = Dash::new(b.diameter, &b.dashes);
                r.dash_ready = true;
            }
            vec![crate::brush::walk_simple_from(
                &c.points,
                false,
                n_tip,
                b.antialiased,
                r.dash.as_mut(),
                Some(&mut r.acc),
            )]
        }
        _ => simple_walks(path, stroke, placement, rect, n_tip),
    };
    let tipw = (tip.n - 1) as f64;
    let half = (n_tip as f64 * 0.5 * 16.0) as i64;
    let edge_mode = !aa && b.kind == BrushType::Simple && placement != StrokePlacement::Center;
    for (ci, walk) in walks.iter().enumerate() {
        let contour = if edge_mode {
            None
        } else {
            path.contours.get(ci).filter(|c| !c.points.is_empty())
        };
        let mut rng = Minstd::new(contour.map_or(0, |c| c.dynamics_at(0).seed).wrapping_add(1));
        let mut last_seg = usize::MAX;
        for s in walk {
            // Stiftzustand: Druck und Geschwindigkeit 16.16 zwischen Segmentanfang und -ende
            let mut st = PenState {
                p: 255,
                spd: 32,
                ..Default::default()
            };
            if let Some(c) = contour {
                let np = c.points.len();
                let i0 = s.seg.min(np - 1);
                let i1 = if s.seg + 1 < np {
                    s.seg + 1
                } else if c.closed {
                    0
                } else {
                    np - 1
                };
                let (d0, d1) = (c.dynamics_at(i0), c.dynamics_at(i1));
                if s.seg != last_seg {
                    last_seg = s.seg;
                    rng = Minstd::new(d0.seed.wrapping_add(1));
                }
                let p0 = (d0.pressure.clamp(0.0, 1.0) as f64 * 255.0) as i64;
                let p1 = (d1.pressure.clamp(0.0, 1.0) as f64 * 255.0) as i64;
                let v0 = sensitivity_speed(d0.velocity) as i64;
                let v1 = sensitivity_speed(d1.velocity) as i64;
                st.p = p0 + (((p1 - p0) * s.frac16) >> 16);
                st.spd = v0 + (((v1 - v0) * s.frac16) >> 16);
            }
            // F26 (re 39 §2.2): Streuung, Stufe, Farbjitter, Deckkraft und Schwärze wie der natürliche
            // Läufer (gemeinsamer Stempel `@788ae0`, Streuung vom Läufer selbst)
            let sv = simple_stamp(b, &st, &mut rng, dp, o_base, color);
            let k = if staged { sv.stage } else { 0 };
            let o = sv.opacity;
            let (mut px16, mut py16) = (s.x + sv.ox * 16 - half, s.y + sv.oy * 16 - half);
            match rgb_ink.as_mut() {
                Some(r) => {
                    r.set_black(sv.black);
                    r.base.note_bounds(px16, py16, tipw);
                }
                None => ink.note_bounds(px16, py16, tipw),
            }
            let (x, y, key) = if !aa {
                if n_tip % 2 == 1 {
                    px16 += 8;
                    py16 += 8;
                }
                (
                    (px16 as f64 / 16.0 + 0.5).floor() as i64,
                    (py16 as f64 / 16.0 + 0.5).floor() as i64,
                    None,
                )
            } else {
                (
                    px16 >> 4,
                    py16 >> 4,
                    Some((k, (px16 & 15) as usize, (py16 & 15) as usize)),
                )
            };
            let staged_tip;
            let mask: &Tip = match key {
                Some(key) => cache
                    .entry(key)
                    .or_insert_with(|| shift_tip(&tip_for(k), key.1, key.2)),
                None if k == 0 => &tip,
                None => {
                    staged_tip = tip_for(k);
                    &staged_tip
                }
            };
            match rgb_ink.as_mut() {
                Some(r) => r.stamp(
                    x,
                    y,
                    mask,
                    o,
                    cap,
                    sv.color,
                    tex_rgb.as_ref().map(|t| (t, b.texture_blend, b.texture_edge)),
                ),
                None => ink.stamp(x, y, mask, o, cap),
            }
        }
    }
    crate::tipcache::keep(
        tip_key,
        crate::tipcache::Tips {
            stages,
            shifted: cache,
        },
    );
}

/// A stroke being drawn, kept between updates: its ink so far and where each walker has got to,
/// so that adding points stamps only the new dabs, however long the stroke already is. The
/// result is the same as drawing the whole stroke with [`stroke_layer`].
#[derive(Clone, Debug)]
pub struct LiveInk {
    rect: IRect,
    data: Vec<u8>,
    rgb: Option<Vec<[u8; 3]>>,
    bounds: Option<(i64, i64, i64, i64)>,
    simple: SimpleResume,
    natural: Option<crate::natural::NaturalResume>,
}

impl LiveInk {
    /// An empty stroke over `rect`.
    pub fn new(rect: IRect) -> Self {
        let (w, h) = (rect.width() as usize, rect.height() as usize);
        Self {
            rect,
            data: vec![0; w * h],
            rgb: None,
            bounds: None,
            simple: SimpleResume::default(),
            natural: None,
        }
    }

    /// Back to an empty stroke.
    pub fn reset(&mut self) {
        self.data.iter_mut().for_each(|v| *v = 0);
        self.rgb = None;
        self.bounds = None;
        self.simple = SimpleResume::default();
        self.natural = None;
    }

    /// Stamp the next piece of the stroke (`path` has one open contour, starting at the previous
    /// piece's last point). Returns the area that may have changed.
    pub fn feed(&mut self, path: &VectorPath, stroke: &Stroke) -> Option<IRect> {
        let b = &stroke.brush;
        if b.feedback == Feedback::Background {
            return None;
        }
        let (w, h) = (self.rect.width() as usize, self.rect.height() as usize);
        let placement = StrokePlacement::Center;
        let mut base = InkBuffer::new(IRect::new(0, 0, 0, 0));
        base.rect = self.rect;
        base.w = w;
        base.h = h;
        base.data = std::mem::take(&mut self.data);
        let natural = b.kind == BrushType::Natural;
        let piece;
        if natural || crate::natural::simple_needs_rgb(b) {
            let rgb = self.rgb.take().unwrap_or_else(|| vec![[255; 3]; w * h]);
            let mut wrapped = Some(crate::natural::ArgbInk::from_parts(base, rgb));
            if natural {
                if let Some(ink) = wrapped.as_mut() {
                    crate::natural::natural_run(path, stroke, placement, ink, Some(&mut self.natural));
                }
            } else {
                let mut unused = InkBuffer::new(IRect::new(0, 0, 0, 0));
                simple_run(
                    path,
                    stroke,
                    placement,
                    self.rect,
                    &mut unused,
                    &mut wrapped,
                    Some(&mut self.simple),
                );
            }
            let ink = wrapped?;
            let (b, rgb) = ink.into_parts();
            piece = b.bounds;
            self.data = b.data;
            self.rgb = Some(rgb);
        } else {
            simple_run(
                path,
                stroke,
                placement,
                self.rect,
                &mut base,
                &mut None,
                Some(&mut self.simple),
            );
            piece = base.bounds;
            self.data = base.data;
        }
        let p = piece?;
        self.bounds = Some(match self.bounds {
            None => p,
            Some(o) => (o.0.min(p.0), o.1.min(p.1), o.2.max(p.2), o.3.max(p.3)),
        });
        // A stamp reaches one cell past its noted rectangle.
        let r = self.rect;
        let area = IRect::new(
            (p.0 - 2).max(r.left as i64) as i32,
            (p.1 - 2).max(r.top as i64) as i32,
            (p.2 + 2).min(r.right as i64) as i32,
            (p.3 + 2).min(r.bottom as i64) as i32,
        );
        (area.right > area.left && area.bottom > area.top).then_some(area)
    }

    /// The stroke as RGBA over `area` (inside the stroke's rectangle), as [`stroke_layer`].
    pub fn layer(&self, stroke: &Stroke, area: IRect) -> Vec<[u8; 4]> {
        let (aw, ah) = (area.width() as usize, area.height() as usize);
        let mut out = vec![[0u8; 4]; aw * ah];
        let Some((bx0, by0, bx1, by1)) = self.bounds else {
            return out;
        };
        let b = &stroke.brush;
        let table =
            (b.alpha_remap != 0).then(|| crate::natural::alpha_remap_table(b.alpha_remap, b.concentration));
        let col = stroke.color;
        let ca = col.a();
        let w = self.rect.width() as usize;
        for j in 0..ah {
            let y = area.top as i64 + j as i64;
            if y < by0 || y >= by1 {
                continue;
            }
            let row = (y - self.rect.top as i64) as usize * w;
            for i in 0..aw {
                let x = area.left as i64 + i as i64;
                if x < bx0 || x >= bx1 {
                    continue;
                }
                let k = row + (x - self.rect.left as i64) as usize;
                let mut a = self.data[k];
                if let Some(t) = &table {
                    a = t[a as usize];
                }
                if a != 0 {
                    let a = if ca != 255 { mul(a, ca) } else { a };
                    let rgb = match &self.rgb {
                        Some(v) => v[k],
                        None => [col.r(), col.g(), col.b()],
                    };
                    out[j * aw + i] = [rgb[0], rgb[1], rgb[2], a];
                }
            }
        }
        out
    }
}

/// Ergebnis eines Läufers: 8-Bit-Strichpuffer über dem Fenster, bei natürlichen Pinseln mit
/// Farbe je Pixel (Farbjitter, Mehrfachspitzen).
pub(crate) struct InkBufferOut {
    pub data: Vec<u8>,
    pub rgb: Option<Vec<[u8; 3]>>,
}

/// `BFBi` als Zahl.
pub(crate) fn feedback_code(f: Feedback) -> u32 {
    match f {
        Feedback::None => 0,
        Feedback::Brush => 1,
        Feedback::Background => 2,
    }
}

/// Stempelpositionen der simple-Pinsel je Teilpfad bzw. Walker-Modus 2 (dort ohne Segment).
pub(crate) fn simple_walks(
    path: &VectorPath,
    stroke: &Stroke,
    placement: StrokePlacement,
    rect: IRect,
    n_tip: i64,
) -> Vec<Vec<SimpleStep>> {
    let b = &stroke.brush;
    let mut walks = Vec::new();
    if !b.antialiased && b.kind == BrushType::Simple && placement != StrokePlacement::Center {
        walks.push(
            edge_walk(path, rect, n_tip)
                .into_iter()
                .map(|(x, y)| SimpleStep {
                    x,
                    y,
                    seg: 0,
                    frac16: 0,
                })
                .collect(),
        );
    } else {
        for c in &path.contours {
            let mut dash = Dash::new(b.diameter, &b.dashes);
            walks.push(walk_simple_steps(
                &c.points,
                c.closed,
                n_tip,
                b.antialiased,
                dash.as_mut(),
            ));
        }
    }
    walks
}

/// Walker-Modus 2 (`@788350`): Pfad hart rastern (Fenster um `N + 2` erweitert), Stempel auf jedes
/// gesetzte Pixel mit ungesetztem 4-Nachbarn (oder am Fensterrand), Position `(x·16, y·16)`.
pub(crate) fn edge_walk(path: &VectorPath, rect: IRect, n_tip: i64) -> Vec<(i64, i64)> {
    let mg = (n_tip + 2).clamp(0, 1 << 20) as i32;
    let big = rect.inflate(mg, mg);
    let polys = path.polys();
    let hm: Coverage = fill_coverage(&polys, big, path.even_odd, path.agm, false);
    let (wx, hx) = (hm.width(), hm.height());
    let mut out = Vec::new();
    for j in 0..hx {
        for i in 0..wx {
            if hm.data[j * wx + i] == 0 {
                continue;
            }
            let edge = i == 0
                || j == 0
                || i == wx - 1
                || j == hx - 1
                || hm.data[j * wx + i - 1] == 0
                || hm.data[j * wx + i + 1] == 0
                || hm.data[(j - 1) * wx + i] == 0
                || hm.data[(j + 1) * wx + i] == 0;
            if edge {
                out.push((
                    (big.left as i64 + i as i64) * 16,
                    (big.top as i64 + j as i64) * 16,
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod stamp_tests {
    use super::*;

    /// The stamp loops spell `mul` out; they must stay the same formula.
    #[test]
    fn the_stamp_loops_inline_mul_exactly() {
        for a in 0..=255u32 {
            for b in 0..=255u32 {
                let t = a * b + 128;
                assert_eq!((t + (t >> 8)) >> 8, u32::from(mul(a as u8, b as u8)), "{a} {b}");
            }
        }
    }
}
