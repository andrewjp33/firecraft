//! Tool and effect settings kept between sessions.
//!
//! Fireworks MX keeps its tools' last settings in its preferences file (`BrushCurrentColor`,
//! `FillCurrentColor`, `WandToolTolerance`, `EraserToolDiameter`, `Text_Font`, ...) and starts the
//! next session with them. FireCraft does the same for everything new objects and the painting
//! tools take from [`ToolOptions`] (colors, stroke category, size and edge, fill category and
//! gradient, textures, text formatting, Eraser/Paint Bucket/Magic Wand/Marquee settings), and for
//! each effect's last settings (the values an effect or filter starts from when next added).
//!
//! Both are `key=value` text, like the preferences: unknown keys are ignored and missing ones keep
//! their defaults, so older and newer versions can share the stored settings.

use std::collections::HashMap;

use firecraft_doc::{EffectEntry, EffectParam};
use firecraft_engine::{MarqueeStyle, ToolOptions};
use firecraft_fwpng::Node;
use firecraft_raster::blend::BlendMode;
use firecraft_raster::gradient::{Gradient, GradientKind};
use firecraft_raster::{Edge, Rgba};

/// Storage key for the tool settings (eframe storage, beside [`crate::Prefs::STORAGE_KEY`]).
pub const TOOLS_KEY: &str = "firecraft.tools";
/// Storage key for the effects' last settings.
pub const EFFECTS_KEY: &str = "firecraft.effects";

/// Textures and the pattern named in stored tool settings: they live in libraries read from the
/// Fireworks folders, so they are looked up once those are read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pending {
    pub stroke_texture: Option<String>,
    pub fill_texture: Option<String>,
    /// The pattern's stored name (file stem).
    pub pattern: Option<String>,
}

fn color_text(c: Option<Rgba>) -> String {
    c.map_or_else(|| "none".to_owned(), Rgba::to_hex)
}

fn parse_color(v: &str) -> Option<Option<Rgba>> {
    if v == "none" {
        Some(None)
    } else {
        Rgba::from_hex(v).map(Some)
    }
}

fn edge_text(e: Edge) -> String {
    match e {
        Edge::Hard => "hard".into(),
        Edge::AntiAlias => "anti-alias".into(),
        Edge::Feather(n) => format!("feather {n}"),
    }
}

fn parse_edge(v: &str) -> Option<Edge> {
    match v {
        "hard" => Some(Edge::Hard),
        "anti-alias" => Some(Edge::AntiAlias),
        _ => v
            .strip_prefix("feather ")
            .and_then(|n| n.trim().parse::<u32>().ok())
            .map(|n| Edge::Feather(n.min(1000))),
    }
}

/// A gradient's stops: `pos:#color,...;pos:alpha,...` (its handles are set per object).
fn stops_text(g: &Gradient) -> String {
    let colors: Vec<String> = g
        .colors
        .iter()
        .map(|(p, c)| format!("{p}:{}", c.to_hex()))
        .collect();
    let opacity: Vec<String> = g.opacity.iter().map(|(p, a)| format!("{p}:{a}")).collect();
    format!("{};{}", colors.join(","), opacity.join(","))
}

#[allow(clippy::type_complexity)]
fn parse_stops(v: &str) -> Option<(Vec<(f32, Rgba)>, Vec<(f32, u8)>)> {
    fn list<T>(s: &str, value: impl Fn(&str) -> Option<T>) -> Option<Vec<(f32, T)>> {
        s.split(',')
            .filter(|x| !x.is_empty())
            .map(|x| {
                let (p, v) = x.split_once(':')?;
                let p: f32 = p.trim().parse().ok().filter(|p: &f32| p.is_finite())?;
                Some((p.clamp(0.0, 1.0), value(v.trim())?))
            })
            .collect()
    }
    let (c, o) = v.split_once(';')?;
    let colors = list(c, Rgba::from_hex)?;
    let opacity = list(o, |a| a.parse::<u8>().ok())?;
    (!colors.is_empty()).then_some((colors, opacity))
}

fn gradient_for(category: &str, stops: &str) -> Option<Gradient> {
    let kind = GradientKind::from_category(category)?;
    let (colors, opacity) = parse_stops(stops)?;
    Some(firecraft_engine::new_gradient(
        kind,
        colors,
        opacity,
        (0.0, 0.0, 1.0, 1.0),
    ))
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect()
}

/// The largest number the Marquee's Style takes (ratio parts, fixed width and height).
pub const MARQUEE_MAX: u32 = 30_000;

/// `a,b`, both in `range`.
fn parse_pair(v: &str, range: std::ops::RangeInclusive<u32>) -> Option<(u32, u32)> {
    let (a, b) = v.split_once(',')?;
    let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (range.contains(&a) && range.contains(&b)).then_some((a, b))
}

/// Text with no line breaks (names and folders).
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], "")
}

/// The tool settings as `key=value` lines.
pub fn serialize_tools(o: &ToolOptions) -> String {
    let p = &o.paint;
    let t = &o.text_style;
    let mut lines: Vec<(&str, String)> = vec![
        ("stroke_color", color_text(o.stroke)),
        ("fill_color", color_text(o.fill)),
        ("stroke_size", o.stroke_size.to_string()),
        ("stroke_softness", o.stroke_softness.to_string()),
        ("stroke_placement", o.stroke_placement.to_string()),
        ("roundness", o.roundness.to_string()),
        ("fill_category", one_line(&o.fill_category)),
        ("fill_anti_alias", o.fill_anti_alias.to_string()),
        ("fill_feather", o.fill_feather.to_string()),
        ("fill_dither_transparent", o.fill_dither_transparent.to_string()),
        ("fill_texture_amount", o.fill_texture_amount.to_string()),
        ("fill_texture_transparent", o.fill_texture_transparent.to_string()),
        ("gradient_category", one_line(&o.gradient_category)),
        ("gradient_tool", stops_text(&o.gradient_tool)),
        ("tolerance", o.tolerance.to_string()),
        ("wand_edge", edge_text(o.wand_edge)),
        ("wand_live", o.wand_live.to_string()),
        ("marquee_edge", edge_text(o.marquee_edge)),
        ("marquee_style", o.marquee_style.code().to_string()),
        ("marquee_ratio", format!("{},{}", o.marquee_ratio.0, o.marquee_ratio.1)),
        ("marquee_size", format!("{},{}", o.marquee_size.0, o.marquee_size.1)),
        ("marquee_live", o.marquee_live.to_string()),
        ("paint_opacity", p.opacity.to_string()),
        ("paint_blend", p.blend.fw_code().to_string()),
        ("paint_preserve_transparency", p.preserve_transparency.to_string()),
        ("pencil_anti_alias", p.pencil_anti_alias.to_string()),
        ("pencil_auto_erase", p.pencil_auto_erase.to_string()),
        ("eraser_size", p.eraser_size.to_string()),
        ("eraser_round", p.eraser_round.to_string()),
        ("eraser_edge", p.eraser_edge.to_string()),
        ("eraser_opacity", p.eraser_opacity.to_string()),
        ("bucket_tolerance", p.bucket_tolerance.to_string()),
        ("bucket_edge", edge_text(p.bucket_edge)),
        ("bucket_fill_selection", p.bucket_fill_selection.to_string()),
        ("text_font", one_line(&t.font)),
        ("text_size", t.size.to_string()),
        ("text_bold", t.bold.to_string()),
        ("text_italic", t.italic.to_string()),
        ("text_underline", t.underline.to_string()),
        ("text_h_scale", t.h_scale.to_string()),
        ("text_tracking", t.tracking.to_string()),
        ("text_baseline_shift", t.baseline_shift.to_string()),
        ("text_leading", t.leading.to_string()),
        ("text_leading_px", t.leading_px.to_string()),
        ("text_align", t.align.to_string()),
        ("text_indent", t.indent.to_string()),
        ("text_space_before", t.space_before.to_string()),
        ("text_space_after", t.space_after.to_string()),
        ("text_anti_alias", o.text_anti_alias.to_string()),
        ("text_auto_kern", o.text_auto_kern.to_string()),
    ];
    if let Some(b) = firecraft_doc::brush_presets::BRUSH_PRESETS.get(o.stroke_preset) {
        lines.push(("stroke_preset", format!("{}/{}", b.category, b.name)));
    }
    if let Some(b) = &o.stroke_brush {
        lines.push(("stroke_brush", hex_bytes(&b.to_bytes())));
    }
    if let Some(g) = &o.fill_gradient {
        lines.push(("fill_gradient", stops_text(g)));
    }
    if let Some(t) = &o.stroke_texture {
        lines.push(("stroke_texture", one_line(&t.name)));
    }
    if let Some(t) = &o.fill_texture {
        lines.push(("fill_texture", one_line(&t.name)));
    }
    if let Some(pat) = &o.fill_pattern {
        lines.push(("fill_pattern", one_line(&pat.name)));
    }
    lines.into_iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

fn set<T: std::str::FromStr>(slot: &mut T, v: &str) {
    if let Ok(x) = v.parse() {
        *slot = x;
    }
}

/// A finite number in `range`.
fn set_f64(slot: &mut f64, v: &str, range: std::ops::RangeInclusive<f64>) {
    if let Ok(x) = v.parse::<f64>()
        && x.is_finite()
    {
        *slot = x.clamp(*range.start(), *range.end());
    }
}

/// Stored tool settings onto `o` (missing or unreadable ones keep its values). Returns the
/// textures and pattern to look up once their libraries are read.
pub fn parse_tools(text: &str, o: &mut ToolOptions) -> Pending {
    let mut pending = Pending::default();
    let mut fill_stops: Option<String> = None;
    let mut gradient_stops: Option<String> = None;
    for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
        let v = v.trim();
        let p = &mut o.paint;
        let t = &mut o.text_style;
        match k.trim() {
            "stroke_color" => o.stroke = parse_color(v).unwrap_or(o.stroke),
            "fill_color" => o.fill = parse_color(v).unwrap_or(o.fill),
            "stroke_size" => {
                set(&mut o.stroke_size, v);
                o.stroke_size = o.stroke_size.clamp(1, 1000);
            }
            "stroke_softness" => {
                set(&mut o.stroke_softness, v);
                o.stroke_softness = o.stroke_softness.min(100);
            }
            "stroke_placement" => {
                set(&mut o.stroke_placement, v);
                o.stroke_placement = o.stroke_placement.min(2);
            }
            "stroke_preset" => {
                if let Some((c, n)) = v.split_once('/')
                    && let Some(i) = firecraft_doc::brush_presets::find(c, n)
                {
                    o.stroke_preset = i;
                }
            }
            "stroke_brush" => {
                o.stroke_brush = unhex(v)
                    .and_then(|b| Node::parse(&b).ok())
                    .filter(|n| n.is("BPL"));
            }
            "roundness" => set_f64(&mut o.roundness, v, 0.0..=1.0),
            "fill_category" if !v.is_empty() => o.fill_category = v.to_owned(),
            "fill_gradient" => fill_stops = Some(v.to_owned()),
            "fill_anti_alias" => set(&mut o.fill_anti_alias, v),
            "fill_feather" => set(&mut o.fill_feather, v),
            "fill_dither_transparent" => set(&mut o.fill_dither_transparent, v),
            "fill_texture_amount" => {
                set(&mut o.fill_texture_amount, v);
                o.fill_texture_amount = o.fill_texture_amount.min(1000);
            }
            "fill_texture_transparent" => set(&mut o.fill_texture_transparent, v),
            "gradient_category" if GradientKind::from_category(v).is_some() => {
                o.gradient_category = v.to_owned();
            }
            "gradient_tool" => gradient_stops = Some(v.to_owned()),
            "tolerance" => set(&mut o.tolerance, v),
            "wand_edge" => o.wand_edge = parse_edge(v).unwrap_or(o.wand_edge),
            "marquee_edge" => o.marquee_edge = parse_edge(v).unwrap_or(o.marquee_edge),
            "wand_live" => set(&mut o.wand_live, v),
            "marquee_live" => set(&mut o.marquee_live, v),
            "marquee_style" => {
                if let Some(st) = v.parse().ok().and_then(MarqueeStyle::from_code) {
                    o.marquee_style = st;
                }
            }
            "marquee_ratio" => o.marquee_ratio = parse_pair(v, 1..=MARQUEE_MAX).unwrap_or(o.marquee_ratio),
            "marquee_size" => o.marquee_size = parse_pair(v, 1..=MARQUEE_MAX).unwrap_or(o.marquee_size),
            "paint_opacity" => {
                set(&mut p.opacity, v);
                p.opacity = p.opacity.min(100);
            }
            "paint_blend" => {
                if let Ok(c) = v.parse::<i64>() {
                    p.blend = BlendMode::from_fw_code(c);
                }
            }
            "paint_preserve_transparency" => set(&mut p.preserve_transparency, v),
            "pencil_anti_alias" => set(&mut p.pencil_anti_alias, v),
            "pencil_auto_erase" => set(&mut p.pencil_auto_erase, v),
            "eraser_size" => {
                set(&mut p.eraser_size, v);
                p.eraser_size = p.eraser_size.clamp(1, 1000);
            }
            "eraser_round" => set(&mut p.eraser_round, v),
            "eraser_edge" => {
                set(&mut p.eraser_edge, v);
                p.eraser_edge = p.eraser_edge.min(100);
            }
            "eraser_opacity" => {
                set(&mut p.eraser_opacity, v);
                p.eraser_opacity = p.eraser_opacity.min(100);
            }
            "bucket_tolerance" => set(&mut p.bucket_tolerance, v),
            "bucket_edge" => p.bucket_edge = parse_edge(v).unwrap_or(p.bucket_edge),
            "bucket_fill_selection" => set(&mut p.bucket_fill_selection, v),
            "text_font" if !v.is_empty() => t.font = v.to_owned(),
            "text_size" => set_f64(&mut t.size, v, 1.0..=1000.0),
            "text_bold" => set(&mut t.bold, v),
            "text_italic" => set(&mut t.italic, v),
            "text_underline" => set(&mut t.underline, v),
            "text_h_scale" => set_f64(&mut t.h_scale, v, 0.01..=100.0),
            "text_tracking" => set_f64(&mut t.tracking, v, -1000.0..=1000.0),
            "text_baseline_shift" => set_f64(&mut t.baseline_shift, v, -1000.0..=1000.0),
            "text_leading" => set_f64(&mut t.leading, v, 0.0..=1000.0),
            "text_leading_px" => set(&mut t.leading_px, v),
            "text_align" => {
                set(&mut t.align, v);
                t.align = t.align.min(4);
            }
            "text_indent" => set_f64(&mut t.indent, v, -1000.0..=1000.0),
            "text_space_before" => set_f64(&mut t.space_before, v, -1000.0..=1000.0),
            "text_space_after" => set_f64(&mut t.space_after, v, -1000.0..=1000.0),
            "text_anti_alias" => {
                set(&mut o.text_anti_alias, v);
                o.text_anti_alias = o.text_anti_alias.min(3);
            }
            "text_auto_kern" => set(&mut o.text_auto_kern, v),
            "stroke_texture" if !v.is_empty() => pending.stroke_texture = Some(v.to_owned()),
            "fill_texture" if !v.is_empty() => pending.fill_texture = Some(v.to_owned()),
            "fill_pattern" if !v.is_empty() => pending.pattern = Some(v.to_owned()),
            _ => {}
        }
    }
    // A gradient category with its stops; a stored category that is a gradient but whose stops
    // are lost starts from the default ones.
    match GradientKind::from_category(&o.fill_category) {
        Some(_) => match fill_stops
            .as_deref()
            .and_then(|s| gradient_for(&o.fill_category, s))
        {
            Some(g) => o.fill_gradient = Some(g),
            None => {
                let category = o.fill_category.clone();
                o.fill_gradient = None;
                o.set_fill_category(&category);
            }
        },
        None => o.fill_gradient = None,
    }
    if let Some(g) = gradient_stops
        .as_deref()
        .and_then(|s| gradient_for(&o.gradient_category, s))
    {
        o.gradient_tool = g;
    }
    pending
}

/// Parameters every effect entry carries that aren't settings (its identity and list state).
const HEADER: &[&str] = &[
    "EffectIsVisible",
    "EffectMoaID",
    "category",
    "name",
    "MB_filter_preview_tile_size",
    "mkbFile_WriteOnly_TemporaryEffectUiName",
];

/// Each effect's last settings, by its menu name ("Drop Shadow", "Gaussian Blur..."): the
/// values a new one starts from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectMemory {
    pub last: HashMap<String, Vec<(String, EffectParam)>>,
}

impl EffectMemory {
    /// Remember `entry`'s settings as the last ones for the effect called `name`.
    pub fn remember(&mut self, name: &str, entry: &EffectEntry) {
        let params: Vec<(String, EffectParam)> = entry
            .params
            .iter()
            .filter(|(k, _)| !HEADER.contains(&k.as_str()))
            .cloned()
            .collect();
        self.last.insert(name.to_owned(), params);
    }

    /// `entry` (a new effect called `name`, with its default settings) with the last settings
    /// used for that effect.
    pub fn apply(&self, name: &str, entry: &mut EffectEntry) {
        let Some(params) = self.last.get(name) else {
            return;
        };
        for (k, v) in params {
            match entry.params.iter_mut().find(|(key, _)| key == k) {
                Some((_, slot)) => *slot = v.clone(),
                None => entry.params.push((k.clone(), v.clone())),
            }
        }
        entry.reparse();
    }

    /// One `name<TAB>key<TAB>value` line per setting; values are `t:` text or `n:` a hex
    /// encoded node (Curves' points), with tabs, line breaks and backslashes escaped.
    pub fn serialize(&self) -> String {
        let mut names: Vec<&String> = self.last.keys().collect();
        names.sort();
        let mut out = String::new();
        for name in names {
            for (k, v) in self.last.get(name).into_iter().flatten() {
                let value = match v {
                    EffectParam::Text(t) => format!("t:{}", escape(t)),
                    EffectParam::Node(n) => format!("n:{}", hex_bytes(&n.to_bytes())),
                };
                out.push_str(&format!("{}\t{}\t{value}\n", escape(name), escape(k)));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Self {
        let mut m = Self::default();
        for line in text.lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(name), Some(key), Some(value)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let value = if let Some(t) = value.strip_prefix("t:") {
                EffectParam::Text(unescape(t))
            } else if let Some(h) = value.strip_prefix("n:")
                && let Some(n) = unhex(h).and_then(|b| Node::parse(&b).ok())
            {
                EffectParam::Node(n)
            } else {
                continue;
            };
            m.last
                .entry(unescape(name))
                .or_default()
                .push((unescape(key), value));
        }
        m
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_round_trip() {
        let mut o = ToolOptions {
            stroke: Some(Rgba::new(1, 2, 3, 255)),
            fill: None,
            stroke_size: 17,
            stroke_softness: 40,
            stroke_placement: 2,
            stroke_preset: firecraft_doc::brush_presets::find("bc_Charcoal", "bn_Creamy").unwrap_or_default(),
            roundness: 0.25,
            tolerance: 99,
            wand_edge: Edge::Feather(7),
            marquee_edge: Edge::Hard,
            marquee_style: MarqueeStyle::FixedSize,
            marquee_ratio: (3, 4),
            marquee_size: (120, 30),
            marquee_live: false,
            wand_live: false,
            text_anti_alias: 2,
            text_auto_kern: false,
            fill_feather: 3,
            ..Default::default()
        };
        o.paint.eraser_size = 42;
        o.paint.eraser_round = false;
        o.paint.blend = BlendMode::Multiply;
        o.paint.bucket_edge = Edge::Feather(4);
        o.text_style.font = "Verdana".into();
        o.text_style.size = 30.5;
        o.text_style.bold = true;
        o.set_fill_category("fc_Elliptical");
        if let Some(g) = &mut o.fill_gradient {
            g.colors = vec![(0.0, Rgba::new(255, 0, 0, 255)), (0.5, Rgba::new(0, 0, 255, 255))];
            g.opacity = vec![(0.0, 255), (1.0, 10)];
        }
        o.gradient_category = "fc_Satin".into();
        o.gradient_tool = firecraft_engine::new_gradient(
            GradientKind::Satin,
            vec![(0.0, Rgba::WHITE), (1.0, Rgba::new(9, 9, 9, 255))],
            vec![(0.0, 255), (0.7, 128)],
            (0.0, 0.0, 1.0, 1.0),
        );
        o.stroke_brush = Some(Node::vector(
            "BPL",
            vec![Node::string("CAT", "bc_Basic"), Node::int("DIA", 5)],
        ));
        let text = serialize_tools(&o);
        let mut back = ToolOptions::default();
        let pending = parse_tools(&text, &mut back);
        assert_eq!(pending, Pending::default());
        assert_eq!(back, o);
    }

    #[test]
    fn tools_pending_assets_and_bad_values() {
        let mut o = ToolOptions::default();
        let pending = parse_tools(
            "stroke_texture=Grain\nfill_pattern=Berber Rug\nstroke_size=abc\nfill_category=fc_Linear\n\
             wand_edge=sideways\nstroke_brush=zz\nunknown=1\n",
            &mut o,
        );
        assert_eq!(pending.stroke_texture.as_deref(), Some("Grain"));
        assert_eq!(pending.pattern.as_deref(), Some("Berber Rug"));
        let d = ToolOptions::default();
        assert_eq!(
            (o.stroke_size, o.wand_edge, o.stroke_brush.is_none()),
            (d.stroke_size, d.wand_edge, true)
        );
        // A gradient category without stops gets the default ones.
        assert!(
            o.fill_gradient
                .as_ref()
                .is_some_and(|g| g.kind == GradientKind::Linear)
        );
    }

    #[test]
    fn effects_round_trip() {
        let mut e = EffectEntry::from_pairs(&[
            ("EffectIsVisible", "true"),
            ("EffectMoaID", "{x}"),
            ("name", "Odd"),
            ("ShadowDistance", "12"),
            ("weird", "a\tb\\c\nd"),
        ]);
        e.set_points("rgb_points", &[(0.0, 0.0), (128.0, 200.0), (255.0, 255.0)]);
        let mut m = EffectMemory::default();
        m.remember("Odd", &e);
        let back = EffectMemory::parse(&m.serialize());
        assert_eq!(back, m);
        let stored = back
            .last
            .get("Odd")
            .map(|p| p.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>());
        assert_eq!(stored, Some(vec!["ShadowDistance", "weird", "rgb_points"]));

        let mut fresh = EffectEntry::from_pairs(&[
            ("EffectIsVisible", "false"),
            ("EffectMoaID", "{x}"),
            ("name", "Odd"),
            ("ShadowDistance", "7"),
        ]);
        back.apply("Odd", &mut fresh);
        assert_eq!(fresh.get("ShadowDistance"), Some("12"));
        assert_eq!(fresh.get("EffectIsVisible"), Some("false"));
        assert_eq!(fresh.points("rgb_points").map(|p| p.len()), Some(3));
    }
}
