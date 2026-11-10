//! Text objects: from the document's model to pixels, with FirePixel's adapted text engine
//! (`firecraft-text`). A text object keeps the image stored with it until it is edited, so a file's
//! appearance survives even when its fonts aren't installed.

use firecraft_doc::element::apply_fill_texture;
use firecraft_doc::{Bitmap, Element, ElementKind, Fill, Text, TextRun};
use firecraft_raster::pattern::web_dither_at;
use firecraft_raster::transform::{Affine, transform_image};
use firecraft_raster::{Rgba, RgbaImage};
use firecraft_text::support::{Rect, Rgba8};
use firecraft_text::{
    Alignment, AntiAlias, CharAttrs, FontDb, FrameKind, LayoutOptions, LeadingMode, Orientation, TextBlock,
    TextLayout,
};

use crate::{DocState, ElementRef, ToolOptions};

fn char_attrs(r: &TextRun) -> CharAttrs {
    CharAttrs {
        font: r.font.clone(),
        size: r.size,
        color: Rgba8::new(r.color.r, r.color.g, r.color.b, r.color.a),
        bold: r.bold,
        italic: r.italic,
        underline: r.underline,
        h_scale: r.h_scale,
        kerning: 0.0,
        // FireCraft keeps an em fraction (Fireworks MX); the layout counts tenths of one.
        tracking: r.tracking * 10.0,
        baseline_shift: r.baseline_shift,
        leading: r.leading,
        leading_mode: if r.leading_px {
            LeadingMode::Exact
        } else {
            LeadingMode::Percentage
        },
        alignment: match r.align {
            1 => Alignment::Center,
            2 => Alignment::Right,
            3 => Alignment::Justify,
            4 => Alignment::Stretch,
            _ => Alignment::Left,
        },
        indent: r.indent,
        space_before: r.space_before,
        space_after: r.space_after,
    }
}

/// The engine's description of a text object.
pub fn text_block(t: &Text) -> TextBlock {
    let (l, top, r, b) = t.rect;
    let (kind, bounds) = match t.anchor {
        Some((x, y)) => (
            FrameKind::Point,
            Rect::new(x as f32, y as f32, x as f32, y as f32),
        ),
        None => (
            FrameKind::FixedWidth,
            Rect::new(l as f32, top as f32, r as f32, b as f32),
        ),
    };
    TextBlock {
        kind,
        bounds,
        runs: t
            .runs
            .iter()
            .map(|run| firecraft_text::TextRun::new(run.text.clone(), char_attrs(run)))
            .collect(),
        auto_kern: t.auto_kern,
        story_kerning: None,
        orientation: Orientation::HorizontalLtr,
        antialias: AntiAlias::from_tags(i64::from(t.anti_alias), 192, 64, 8, None),
        path: None,
        em_box_lines: !t.cs_layout,
    }
}

/// A text object's layout, for carets, selections and hit testing.
pub fn layout_of(t: &Text) -> TextLayout {
    firecraft_text::layout(&text_block(t), &FontDb::shared(), &LayoutOptions::default())
}

/// A canvas point in the text block's own space (before its matrix).
pub fn to_block(t: &Text, x: f64, y: f64) -> (f64, f64) {
    if t.matrix == Affine::IDENTITY {
        return (x, y);
    }
    t.matrix.invert().map_or((x, y), |m| m.apply(x, y))
}

/// A point in the text block's own space on the canvas.
pub fn from_block(t: &Text, x: f64, y: f64) -> (f64, f64) {
    t.matrix.apply(x, y)
}

/// The text drawn with the fonts available here, through its matrix.
pub fn rasterize(t: &Text) -> Option<Bitmap> {
    let mut layout = layout_of(t);
    let m = t.matrix;
    let warp = m != Affine::IDENTITY && !m.is_affine();
    // Glyph outlines follow an affine matrix exactly; a perspective one bends the flat drawing.
    if m != Affine::IDENTITY && m.is_affine() {
        let k = m.i;
        let tm = firecraft_text::raster::Affine {
            a: m.a / k,
            b: m.b / k,
            c: m.c / k,
            d: m.d / k,
            e: m.e / k,
            f: m.f / k,
        };
        for g in &mut layout.glyphs {
            g.transform = g.transform.then(&tm);
        }
        for d in &mut layout.decorations {
            for p in &mut d.quad {
                *p = tm.apply(*p);
            }
        }
    }
    let img = firecraft_text::render::render(&layout, None);
    let b = img.bounds();
    let (w, h) = (img.width() as u32, img.height() as u32);
    // Spaces alone have no pixels.
    if w == 0 || h == 0 {
        return None;
    }
    let mut image = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    for (i, p) in img.data().iter().enumerate() {
        image.set(
            (i as u32 % w.max(1)) as i64,
            (i as u32 / w.max(1)) as i64,
            Rgba::new(p[0], p[1], p[2], p[3]),
        );
    }
    let (x, y) = (i64::from(b.left), i64::from(b.top));
    let mut drawn = if warp {
        let (image, x, y) = transform_image(&image, x, y, &m)?;
        Bitmap { x, y, image }
    } else {
        Bitmap { x, y, image }
    };
    if let Some(f) = &t.fill {
        shade(&mut drawn, f);
    }
    Some(drawn)
}

/// Paint a gradient, pattern or web dither fill through the text's coverage in `b`, sampled at
/// each pixel's canvas coordinate as for paths, then apply the fill's texture.
fn shade(b: &mut Bitmap, f: &Fill) {
    if f.gradient.is_some() || f.pattern.is_some() || f.dither.is_some() {
        let w = b.image.width().max(1) as i64;
        let (x0, y0) = (b.x, b.y);
        for (i, p) in b.image.pixels_mut().iter_mut().enumerate() {
            if p.a == 0 {
                continue;
            }
            let (cx, cy) = (x0 + i as i64 % w, y0 + i as i64 / w);
            let (fx, fy) = (cx as f64, cy as f64);
            let c = match (&f.gradient, &f.pattern, &f.dither) {
                (Some(g), _, _) => g.color_at(fx, fy),
                (_, Some(q), _) => q.color_at(fx, fy),
                (_, _, Some(tile)) => {
                    let c = web_dither_at(tile, fx, fy);
                    if f.dither_transparent && (cx + cy) % 2 != 0 {
                        Rgba { a: 0, ..c }
                    } else {
                        c
                    }
                }
                _ => *p,
            };
            let a = (u32::from(p.a) * u32::from(c.a) + 127) / 255;
            *p = Rgba { a: a as u8, ..c };
        }
    }
    apply_fill_texture(&mut b.image, f, (0.0, 0.0), (b.x, b.y));
}

/// Whether one of the text's fonts isn't here (a replacement would draw it).
fn fonts_missing(t: &Text) -> bool {
    let fonts = FontDb::shared();
    t.runs.iter().any(|r| {
        !fonts
            .resolve(&r.font, r.bold, r.italic)
            .is_some_and(|f| !f.substituted)
    })
}

/// Show a change to the text's fill: drawn again, or, while one of its fonts is missing and it
/// hasn't been edited, its stored glyphs shaded afresh (so it keeps Fireworks' appearance).
pub fn restyle(t: &mut Text) {
    if !t.edited && fonts_missing(t) {
        let color = t.runs.first().map_or(t.color, |r| r.color);
        if let Some(b) = &mut t.cached {
            for p in b.image.pixels_mut() {
                *p = Rgba { a: p.a, ..color };
            }
            if let Some(f) = &t.fill {
                shade(b, f);
            }
            return;
        }
    }
    refresh(t);
}

/// Formatting for new text.
pub fn default_style() -> TextRun {
    TextRun {
        text: String::new(),
        font: "Arial".into(),
        size: 12.0,
        color: Rgba::BLACK,
        bold: false,
        italic: false,
        underline: false,
        h_scale: 1.0,
        tracking: 0.0,
        baseline_shift: 0.0,
        leading: 1.0,
        leading_px: false,
        align: 0,
        indent: 0.0,
        space_before: 0.0,
        space_after: 0.0,
    }
}

/// The name to show and save for a font: its family, where `name` is a regular face's PostScript
/// name (`ArialMT` is Arial). Bold and italic faces keep theirs, which also says the style.
pub fn family_of(name: &str) -> String {
    let fonts = FontDb::shared();
    fonts
        .find_ps(name)
        .and_then(|id| fonts.faces().get(id))
        .filter(|f| !f.bold && !f.italic && !f.names.family.is_empty())
        .map_or_else(|| name.to_owned(), |f| f.names.family.clone())
}

/// How many characters the text has.
pub fn char_count(t: &Text) -> usize {
    t.runs.iter().map(|r| r.text.chars().count()).sum()
}

/// Make `i` a run boundary: the index of the run starting there.
fn split_at(t: &mut Text, i: usize) -> usize {
    let mut at = 0;
    for k in 0..t.runs.len() {
        let n = t.runs[k].text.chars().count();
        if i == at {
            return k;
        }
        if i < at + n {
            let cut: usize = t.runs[k].text.chars().take(i - at).map(char::len_utf8).sum();
            let tail = t.runs[k].text.split_off(cut);
            let mut next = t.runs[k].clone();
            next.text = tail;
            t.runs.insert(k + 1, next);
            return k + 1;
        }
        at += n;
    }
    t.runs.len()
}

/// Tidy runs and the object's summary of them: neighbours that match merge, and one (possibly
/// empty) run always remains to carry the formatting.
fn normalize(t: &mut Text) {
    let keep = t.runs.first().cloned();
    t.runs.retain(|r| !r.text.is_empty());
    let mut merged: Vec<TextRun> = Vec::with_capacity(t.runs.len());
    for r in t.runs.drain(..) {
        match merged.last_mut() {
            Some(last)
                if TextRun {
                    text: String::new(),
                    ..last.clone()
                } == TextRun {
                    text: String::new(),
                    ..r.clone()
                } =>
            {
                last.text.push_str(&r.text);
            }
            _ => merged.push(r),
        }
    }
    if merged.is_empty()
        && let Some(mut first) = keep
    {
        first.text.clear();
        merged.push(first);
    }
    t.runs = merged;
    t.text = t.runs.iter().map(|r| r.text.as_str()).collect();
    if let Some(r) = t.runs.first() {
        t.font.clone_from(&r.font);
        t.size = r.size;
        t.color = r.color;
        t.bold = r.bold;
        t.italic = r.italic;
        t.underline = r.underline;
    }
}

/// Formatting at a caret position: of the character before it, as typing continues it.
pub fn style_at(t: &Text, i: usize) -> TextRun {
    let mut at = 0;
    let mut style = t.runs.first().cloned().unwrap_or_else(default_style);
    for r in &t.runs {
        if at >= i && at > 0 {
            break;
        }
        style = r.clone();
        at += r.text.chars().count();
    }
    style.text.clear();
    style
}

/// Insert characters at `at`, in `style` or the formatting there.
pub fn insert(t: &mut Text, at: usize, s: &str, style: Option<&TextRun>) {
    let mut run = style.cloned().unwrap_or_else(|| style_at(t, at));
    run.text = s.to_owned();
    let k = split_at(t, at);
    t.runs.insert(k, run);
}

/// Remove characters `range`.
pub fn delete(t: &mut Text, range: std::ops::Range<usize>) {
    let start = split_at(t, range.start);
    let end = split_at(t, range.end).max(start);
    let end = end.min(t.runs.len());
    let keep = t.runs.first().cloned();
    t.runs.drain(start.min(end)..end);
    if t.runs.is_empty()
        && let Some(mut run) = keep
    {
        run.text.clear();
        t.runs.push(run);
    }
}

/// Format characters `range`.
pub fn format(t: &mut Text, range: std::ops::Range<usize>, f: &dyn Fn(&mut TextRun)) {
    let start = split_at(t, range.start);
    let end = split_at(t, range.end).max(start);
    let count = t.runs.len();
    for r in &mut t.runs[start.min(count)..end.min(count)] {
        f(r);
    }
}

/// Redraw a text object after a change, and size its block to it.
pub fn refresh(t: &mut Text) {
    for run in &mut t.runs {
        run.font = family_of(&run.font);
    }
    normalize(t);
    t.edited = true;
    if t.anchor.is_some() {
        let f = layout_of(t).frame;
        if !t.text.is_empty() {
            t.rect = (
                f64::from(f.left),
                f64::from(f.top),
                f64::from(f.right),
                f64::from(f.bottom),
            );
        }
        t.cached = if t.text.is_empty() { None } else { rasterize(t) };
        return;
    }
    if t.auto_width {
        let mut wide = t.clone();
        wide.rect.2 = wide.rect.0 + 1e6;
        let measured = layout_of(&wide).lines.iter().map(|l| l.width).fold(0.0, f64::max);
        t.rect.2 = t.rect.0 + measured.ceil().max(1.0);
    }
    let bottom = f64::from(layout_of(t).frame.bottom);
    t.rect.3 = if bottom > t.rect.1 {
        bottom
    } else {
        t.rect.1 + t.size
    };
    t.cached = if t.text.is_empty() { None } else { rasterize(t) };
}

/// Changing only the colour of text whose font is missing keeps its glyphs: the stored image is
/// recoloured rather than redrawn in a replacement font. Whether it did that.
fn recolor_stored_image(before: &Text, t: &mut Text) -> bool {
    let plain = |x: &Text| -> Vec<TextRun> {
        x.runs
            .iter()
            .map(|r| TextRun {
                color: Rgba::BLACK,
                ..r.clone()
            })
            .collect()
    };
    let Some(color) = t.runs.first().map(|r| r.color) else {
        return false;
    };
    if !fonts_missing(t)
        || t.fill
            .as_ref()
            .is_some_and(|f| f.gradient.is_some() || f.pattern.is_some() || f.dither.is_some())
        || before.cached.is_none()
        || plain(before) != plain(t)
        || before.rect != t.rect
        || t.runs.iter().any(|r| r.color != color)
    {
        return false;
    }
    if let Some(b) = &mut t.cached {
        for p in b.image.pixels_mut() {
            *p = Rgba { a: p.a, ..color };
        }
    }
    t.color = color;
    t.edited = true;
    true
}

/// A new, empty text object. `auto`: the block grows with the text, else `rect`'s width wraps it.
pub fn new_text(rect: (f64, f64, f64, f64), auto: bool, opts: &ToolOptions) -> Text {
    let mut style = opts.text_style.clone();
    style.text.clear();
    style.color = opts.fill.unwrap_or(style.color);
    Text {
        runs: vec![style.clone()],
        rect,
        auto_kern: opts.text_auto_kern,
        anti_alias: opts.text_anti_alias,
        auto_width: auto,
        cs_layout: false,
        matrix: Affine::IDENTITY,
        fill: None,
        anchor: None,
        on_path: false,
        edited: true,
        text: String::new(),
        font: style.font,
        size: style.size,
        color: style.color,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        cached: None,
    }
}

/// The fonts a document's text asks for that aren't installed here.
pub fn missing_fonts(d: &DocState) -> Vec<String> {
    fn visit(e: &Element, fonts: &FontDb, out: &mut Vec<String>) {
        match &e.kind {
            ElementKind::Text(t) => {
                for r in &t.runs {
                    let found = fonts
                        .resolve(&r.font, r.bold, r.italic)
                        .is_some_and(|f| !f.substituted);
                    if !found && !out.contains(&r.font) {
                        out.push(r.font.clone());
                    }
                }
            }
            ElementKind::Group(c) => c.iter().for_each(|x| visit(x, fonts, out)),
            ElementKind::Masked { content, .. } => content.iter().for_each(|x| visit(x, fonts, out)),
            _ => {}
        }
    }
    let fonts = FontDb::shared();
    let mut out = Vec::new();
    let layers = d
        .doc
        .layers
        .iter()
        .chain(d.doc.symbols.iter().flat_map(|s| s.layers.iter()));
    for l in layers {
        l.elements(0).iter().for_each(|e| visit(e, &fonts, &mut out));
    }
    out.sort();
    out
}

impl DocState {
    /// The text object at `r`.
    pub fn text_at(&self, r: ElementRef) -> Option<&Text> {
        match self.element(r).map(|e| &e.kind) {
            Some(ElementKind::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// The text object at `r`, if the Text tool can edit it.
    pub fn editable_text_at(&self, r: ElementRef) -> Option<&Text> {
        self.text_at(r).filter(|t| !t.on_path)
    }

    /// Start a new text object, selected, as one History step ending with
    /// [`finish_text`](Self::finish_text).
    pub fn start_new_text(&mut self, t: Text) -> ElementRef {
        self.begin_live_edit("Text");
        let layer = self.writable_layer();
        let frame = self.frame();
        let mut index = 0;
        if let Some(list) = self.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(frame)) {
            list.push(Element::new(ElementKind::Text(t)));
            index = list.len() - 1;
        }
        let r = ElementRef::new(layer, index);
        self.selection = vec![r];
        self.pixels = None;
        self.mark_changed();
        r
    }

    /// Change a text object during editing and redraw it.
    pub fn change_text(&mut self, r: ElementRef, f: impl FnOnce(&mut Text)) {
        if let Some(Element {
            kind: ElementKind::Text(t),
            ..
        }) = self.element_mut(r)
        {
            let before = t.clone();
            f(t);
            if !recolor_stored_image(&before, t) {
                refresh(t);
            }
        }
        self.mark_changed();
    }

    /// Call `f` on every text object in the document and its symbols.
    fn for_each_text(&mut self, f: &mut dyn FnMut(&mut Text)) {
        fn visit(e: &mut Element, f: &mut dyn FnMut(&mut Text)) {
            match &mut e.kind {
                ElementKind::Text(t) => f(t),
                ElementKind::Group(c) => c.iter_mut().for_each(|x| visit(x, f)),
                ElementKind::Masked { content, .. } => content.iter_mut().for_each(|x| visit(x, f)),
                _ => {}
            }
        }
        let frames = self.doc.frames.len().max(1);
        for layer in &mut self.doc.layers {
            for frame in 0..frames {
                if let Some(list) = layer.elements_mut(frame) {
                    list.iter_mut().for_each(|e| visit(e, f));
                }
            }
        }
        for symbol in std::sync::Arc::make_mut(&mut self.doc.symbols) {
            let frames = symbol.frames.max(1);
            for layer in &mut symbol.layers {
                for frame in 0..frames {
                    if let Some(list) = layer.elements_mut(frame) {
                        list.iter_mut().for_each(|e| visit(e, f));
                    }
                }
            }
        }
    }

    /// Use other fonts in place of missing ones throughout the document: (missing, replacement).
    pub fn replace_fonts(&mut self, map: &[(String, String)]) {
        self.for_each_text(&mut |t| {
            let mut changed = false;
            for run in &mut t.runs {
                if let Some((_, to)) = map.iter().find(|(from, _)| *from == run.font) {
                    run.font.clone_from(to);
                    changed = true;
                }
            }
            if changed {
                refresh(t);
            }
        });
        self.mark_changed();
    }

    /// Draw edited text again because more fonts have arrived. Text that hasn't been edited keeps
    /// its stored image, and the document doesn't count as changed.
    pub fn redraw_edited_text(&mut self) {
        let mut any = false;
        self.for_each_text(&mut |t| {
            if t.edited && !t.text.is_empty() {
                refresh(t);
                any = true;
            }
        });
        if any {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// End editing a text object: empty text goes away; `changed`: keep one History step.
    pub fn finish_text(&mut self, r: ElementRef, is_new: bool, changed: bool) {
        let empty = self.text_at(r).is_none_or(|t| t.text.is_empty());
        if is_new && empty {
            self.cancel_live_edit();
            self.selection.clear();
            self.mark_changed();
            return;
        }
        if empty {
            // Text emptied by editing is deleted, as one step from how it was.
            self.cancel_live_edit();
            self.selection = vec![r];
            self.delete();
            self.selection.clear();
            return;
        }
        if changed || is_new {
            self.commit_live_edit();
        } else {
            self.discard_live_edit();
        }
    }

    /// Change the selected text objects' own settings, one History step.
    pub fn change_selected_text_objects(&mut self, label: &str, f: &dyn Fn(&mut Text)) {
        let refs = self.selection.clone();
        self.edit(label, |s| {
            for r in refs {
                if s.editable_text_at(r).is_some() {
                    s.change_text(r, f);
                }
            }
        });
    }

    /// Format the selected text objects, whole (`live`: part of a drag already begun).
    pub fn format_selected_text(&mut self, label: &str, live: bool, f: &dyn Fn(&mut TextRun)) {
        let refs: Vec<ElementRef> = self
            .selection
            .iter()
            .copied()
            .filter(|r| self.editable_text_at(*r).is_some())
            .collect();
        let apply = |s: &mut DocState| {
            for r in &refs {
                s.change_text(*r, |t| {
                    let n = char_count(t);
                    format(t, 0..n, f);
                    if n == 0 {
                        t.runs.iter_mut().for_each(f);
                    }
                });
            }
        };
        if live {
            apply(self);
        } else {
            self.edit(label, apply);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Text {
        let mut t = new_text((0.0, 0.0, 100.0, 20.0), true, &ToolOptions::default());
        insert(&mut t, 0, "Hello world", None);
        refresh(&mut t);
        t
    }

    #[test]
    fn typing_and_deleting_edit_the_text() {
        let mut t = sample();
        insert(&mut t, 5, ",", None);
        refresh(&mut t);
        assert_eq!(t.text, "Hello, world");
        delete(&mut t, 0..7);
        refresh(&mut t);
        assert_eq!(t.text, "world");
        assert_eq!(t.runs.len(), 1);
    }

    #[test]
    fn formatting_part_of_the_text_splits_and_merges_runs() {
        let mut t = sample();
        format(&mut t, 6..11, &|r| r.bold = true);
        refresh(&mut t);
        assert_eq!(t.runs.len(), 2);
        assert_eq!(
            (t.runs[0].text.as_str(), t.runs[1].text.as_str()),
            ("Hello ", "world")
        );
        assert!(!t.runs[0].bold && t.runs[1].bold);
        format(&mut t, 0..11, &|r| r.bold = true);
        refresh(&mut t);
        assert_eq!(t.runs.len(), 1, "matching runs merge");
    }

    #[test]
    fn deleting_across_runs_keeps_the_rest() {
        let mut t = sample();
        format(&mut t, 2..4, &|r| r.italic = true);
        delete(&mut t, 1..9);
        refresh(&mut t);
        assert_eq!(t.text, "Hld");
    }

    #[test]
    fn point_text_stays_on_its_baseline_when_edited() {
        // Fireworks CS point text: a one-glyph object must not take the area it covers for a block.
        let mut t = sample();
        t.auto_width = false;
        t.anchor = Some((170.0, 120.0));
        t.rect = (170.0, 53.0, 213.0, 146.0);
        format(&mut t, 0..11, &|r| r.size = 78.0);
        refresh(&mut t);
        let image = t.cached.as_ref().expect("an image");
        assert!(t.rect.0 <= 171.0 && t.rect.2 > 400.0, "no wrapping: {:?}", t.rect);
        assert!((image.y as f64) < 120.0 && image.y as f64 + f64::from(image.image.height()) > 120.0);
    }

    #[test]
    fn text_with_nothing_to_draw_has_no_image() {
        let mut t = new_text((0.0, 0.0, 100.0, 20.0), true, &ToolOptions::default());
        insert(&mut t, 0, "   ", None);
        refresh(&mut t);
        assert_eq!(t.text, "   ");
        assert!(
            t.cached
                .as_ref()
                .is_none_or(|b| b.image.width() > 0 && b.image.height() > 0)
        );
    }

    #[test]
    fn emptied_text_keeps_its_formatting() {
        let mut t = sample();
        format(&mut t, 0..11, &|r| r.size = 40.0);
        let n = char_count(&t);
        delete(&mut t, 0..n);
        refresh(&mut t);
        assert_eq!((t.runs.len(), t.runs[0].size), (1, 40.0));
        assert!(t.cached.is_none());
    }
}
