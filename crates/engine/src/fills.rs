//! Fill categories, gradients and fill edges: Fireworks MX's defaults (measured by reading each
//! fill back through its scripting API; docs/behavior/gradients.md) and the commands that edit them.

use firecraft_doc::{Element, ElementKind, Fill};
use firecraft_raster::Rgba;
use firecraft_raster::RgbaImage;
use firecraft_raster::gradient::{Gradient, GradientKind};
use firecraft_raster::pattern::PatternFill;

use crate::{DocState, ElementRef, ToolOptions};

/// The gradient fills, in the order of Fireworks MX's Fill Options menu: (category, label).
pub const GRADIENT_FILLS: [(&str, &str); 11] = [
    ("fc_Linear", "Linear"),
    ("fc_Circular", "Radial"),
    ("fc_Elliptical", "Ellipse"),
    ("fc_Rectangular", "Rectangle"),
    ("fc_Conical", "Cone"),
    ("fc_Pinch", "Starburst"),
    ("fc_Bars", "Bars"),
    ("fc_Ripple", "Ripples"),
    ("fc_Waves", "Waves"),
    ("fc_Satin", "Satin"),
    ("fc_Folds", "Folds"),
];

/// The label for a fill category.
pub fn fill_label(category: &str) -> &'static str {
    match category {
        PATTERN => "Pattern",
        WEB_DITHER => "Web Dither",
        _ => GRADIENT_FILLS
            .iter()
            .find(|(c, _)| *c == category)
            .map_or("Solid", |(_, l)| l),
    }
}

/// The preset a gradient's stops are, if any (Fireworks MX's Preset list shows it, and is blank
/// once the stops are changed).
pub fn preset_name(g: &Gradient) -> Option<&'static str> {
    if g.opacity.iter().any(|(_, a)| *a != 255) {
        return None;
    }
    GRADIENT_PRESETS
        .iter()
        .find(|(_, stops)| {
            let colors = preset_colors(stops);
            colors.len() == g.colors.len()
                && colors
                    .iter()
                    .zip(&g.colors)
                    .all(|(a, b)| (a.0 - b.0).abs() < 1e-3 && a.1 == b.1)
        })
        .map(|(name, _)| *name)
}

/// The Pattern fill category.
pub const PATTERN: &str = "fc_Pattern";
/// The Web Dither fill category.
pub const WEB_DITHER: &str = "fc_WebDither";

/// Fireworks MX's Web Dither tile for a colour: per channel, the websafe levels (multiples of
/// 0x33) either side of it, or the nearer one alone within a quarter of a step of it.
pub fn web_dither_tile(c: Rgba) -> [Rgba; 4] {
    let levels = |v: u8| {
        let (step, rest) = (u32::from(v) / 51, u32::from(v) % 51);
        let low = (step * 51) as u8;
        let high = ((step + 1) * 51).min(255) as u8;
        match rest * 4 {
            r if r < 51 => (low, low),
            r if r > 153 => (high, high),
            _ => (low, high),
        }
    };
    let (r, g, b) = (levels(c.r), levels(c.g), levels(c.b));
    let (first, second) = (Rgba::new(r.0, g.0, b.0, 255), Rgba::new(r.1, g.1, b.1, 255));
    [first, second, second, first]
}

/// The websafe colour nearest `c`.
fn websafe_nearest(c: Rgba) -> Rgba {
    let level = |v: u8| ((u32::from(v) + 25) / 51 * 51) as u8;
    Rgba::new(level(c.r), level(c.g), level(c.b), 255)
}

/// The Web Dither tile for a colour, with or without Transparent.
pub(crate) fn dither_tile(c: Rgba, transparent: bool) -> [Rgba; 4] {
    if transparent {
        [websafe_nearest(c); 4]
    } else {
        web_dither_tile(c)
    }
}

/// A pattern placed on an object (bounds x, y, w, h): pixel (0, 0) at its centre and one tile
/// along each handle (Fireworks MX).
pub fn new_pattern(name: &str, image: RgbaImage, (x, y, w, h): (f64, f64, f64, f64)) -> PatternFill {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (tw, th) = (f64::from(image.width()), f64::from(image.height()));
    PatternFill {
        name: name.to_owned(),
        image,
        start: (cx, cy),
        end: (cx + tw, cy),
        side: (cx, cy - th),
    }
}

/// A selected fill's handles, with whether the third is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillHandles {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub side: (f64, f64),
    pub uses_side: bool,
}

/// Fireworks MX's preset gradients: (label, stops of (position, 0xRRGGBB)).
pub const GRADIENT_PRESETS: [(&str, &[(f32, u32)]); 13] = [
    ("Black, White", &[(0.0, 0x000000), (1.0, 0xffffff)]),
    (
        "Blue, Red, Yellow",
        &[(0.0, 0x0900b2), (0.5, 0xff0000), (1.0, 0xfffc00)],
    ),
    (
        "Blue, Yellow, Blue",
        &[(0.1, 0x0b02aa), (0.5, 0xfdfb02), (0.9, 0x0b02aa)],
    ),
    (
        "Cobalt Blue",
        &[
            (0.0, 0x0038ff),
            (0.28, 0x0029a7),
            (0.45, 0x00b6ff),
            (0.6, 0x0029a7),
            (0.75, 0x00b6ff),
            (1.0, 0x0038ff),
        ],
    ),
    (
        "Copper",
        &[
            (0.0, 0x97461a),
            (0.3, 0xfbd8c5),
            (0.83, 0x6c2e16),
            (1.0, 0xefdbcd),
        ],
    ),
    (
        "Emerald Green",
        &[
            (0.0, 0x203c00),
            (0.28, 0x1b971b),
            (0.45, 0x1e4f45),
            (0.6, 0x6aec01),
            (0.75, 0x1b971b),
            (1.0, 0x203c00),
        ],
    ),
    (
        "Pastels",
        &[
            (0.0, 0xff6666),
            (0.28, 0x8080ff),
            (0.45, 0x66ffff),
            (0.6, 0x80ff7f),
            (0.75, 0xffff66),
            (1.0, 0xff7ff7),
        ],
    ),
    ("Red, Blue", &[(0.0, 0xff0000), (1.0, 0x0900b2)]),
    (
        "Red, Green, Blue",
        &[(0.0, 0xff0000), (0.5, 0x00ff00), (1.0, 0x0000ff)],
    ),
    (
        "Silver",
        &[
            (0.0, 0x939090),
            (0.28, 0x424242),
            (0.45, 0xb5b3b1),
            (0.6, 0x666666),
            (0.75, 0xdedede),
            (1.0, 0x808080),
        ],
    ),
    (
        "Spectrum",
        &[
            (0.0, 0xff0000),
            (0.2, 0xff00ff),
            (0.4, 0x002aff),
            (0.6, 0x00ff00),
            (0.8, 0xffff00),
            (1.0, 0xff0000),
        ],
    ),
    ("Violet, Orange", &[(0.0, 0x290a59), (1.0, 0xff7c00)]),
    ("White, Black", &[(0.0, 0xffffff), (1.0, 0x000000)]),
];

/// A preset's colour stops.
/// A plain fill of one colour.
pub fn solid_fill(color: Rgba) -> Fill {
    Fill {
        color,
        category: "fc_Solid".into(),
        anti_alias: true,
        feather: 0,
        pattern: None,
        dither: None,
        dither_transparent: false,
        gradient: None,
        texture: None,
        texture_amount: 0,
        texture_transparent: false,
    }
}

pub fn preset_colors(stops: &[(f32, u32)]) -> Vec<(f32, Rgba)> {
    stops
        .iter()
        .map(|&(p, c)| (p, Rgba::new((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)))
        .collect()
}

/// Where Fireworks MX puts a new gradient's handles, for an object with bounds (x, y, w, h):
/// (start, end, side).
pub fn default_handles(
    kind: GradientKind,
    (x, y, w, h): (f64, f64, f64, f64),
) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let c = (x + w / 2.0, y + h / 2.0);
    let right = (x + w, c.1);
    let up = (c.0, y);
    let quarter = (c.0 + w / 4.0, c.1 + h / 4.0);
    match kind {
        GradientKind::Linear => ((x, c.1), right, (x, c.1 + w)),
        GradientKind::Bars => (c, right, (c.0, c.1 + w / 2.0)),
        GradientKind::Pinch | GradientKind::Starburst => (c, quarter, (c.0 + w / 4.0, c.1 - h / 4.0)),
        GradientKind::Waves => (c, quarter, up),
        GradientKind::Satin | GradientKind::Folds => (c, (x + w, y + h), up),
        _ => (c, right, up),
    }
}

/// A new gradient of `kind` over `bounds`, with these stops.
pub fn new_gradient(
    kind: GradientKind,
    colors: Vec<(f32, Rgba)>,
    opacity: Vec<(f32, u8)>,
    bounds: (f64, f64, f64, f64),
) -> Gradient {
    let (start, end, side) = default_handles(kind, bounds);
    Gradient {
        kind,
        colors,
        opacity,
        start,
        end,
        side,
    }
}

/// Fully opaque.
pub fn opaque_ramp() -> Vec<(f32, u8)> {
    vec![(0.0, 255), (1.0, 255)]
}

/// The stops of a first gradient: from the stroke colour to the fill colour (Fireworks MX).
pub fn default_stops(opts: &ToolOptions, fill: Rgba) -> Vec<(f32, Rgba)> {
    vec![(0.0, opts.stroke.unwrap_or(Rgba::BLACK)), (1.0, fill)]
}

impl ToolOptions {
    /// A new fill with the tool settings for an object with these bounds; `None` for no fill.
    pub fn new_fill(&self, bounds: (f64, f64, f64, f64)) -> Option<Fill> {
        let color = self.fill?;
        let gradient = self
            .fill_gradient
            .as_ref()
            .map(|g| new_gradient(g.kind, g.colors.clone(), g.opacity.clone(), bounds));
        let pattern = (self.fill_category == PATTERN)
            .then(|| {
                let p = self.fill_pattern.clone()?;
                Some(new_pattern(&p.name, p.image, bounds))
            })
            .flatten();
        Some(Fill {
            color,
            category: self.fill_category.clone(),
            anti_alias: self.fill_anti_alias,
            feather: self.fill_feather,
            pattern,
            dither: (self.fill_category == WEB_DITHER)
                .then(|| dither_tile(color, self.fill_dither_transparent)),
            dither_transparent: self.fill_dither_transparent,
            gradient,
            texture: self.fill_texture.clone(),
            texture_amount: self.fill_texture_amount,
            texture_transparent: self.fill_texture_transparent,
        })
    }

    /// Take on a selected object's fill: new objects get it too (Fireworks MX).
    pub fn adopt_fill(&mut self, fill: Option<&Fill>) {
        let Some(f) = fill else {
            self.fill = None;
            self.fill_category = "fc_Solid".into();
            self.fill_gradient = None;
            return;
        };
        self.fill = Some(f.color);
        self.fill_category.clone_from(&f.category);
        self.fill_gradient.clone_from(&f.gradient);
        if f.pattern.is_some() {
            self.fill_pattern.clone_from(&f.pattern);
        }
        self.fill_dither_transparent = f.dither_transparent;
        self.fill_anti_alias = f.anti_alias;
        self.fill_feather = f.feather;
        if f.texture.is_some() {
            self.fill_texture.clone_from(&f.texture);
        }
        self.fill_texture_amount = f.texture_amount;
        self.fill_texture_transparent = f.texture_transparent;
    }

    /// Take on a selected object's stroke: new objects get it too (Fireworks MX), none included.
    pub fn adopt_stroke(&mut self, stroke: Option<&firecraft_doc::Stroke>) {
        let Some(s) = stroke else {
            self.stroke = None;
            return;
        };
        self.stroke = Some(s.color);
        self.stroke_size = s.diameter.round().max(1.0) as u32;
        self.stroke_softness = s.softness;
        self.stroke_placement = s.placement;
        if let Some(i) = firecraft_doc::brush_presets::find(&s.category, &s.name) {
            self.stroke_preset = i;
        }
        self.stroke_brush.clone_from(&s.brush);
        if s.texture.is_some() {
            self.stroke_texture.clone_from(&s.texture);
        }
    }

    /// Change the kind of fill new objects get.
    pub fn set_fill_category(&mut self, category: &str) {
        self.fill_category = category.to_owned();
        match GradientKind::from_category(category) {
            Some(kind) => {
                let (colors, opacity) = match &self.fill_gradient {
                    Some(g) => (g.colors.clone(), g.opacity.clone()),
                    None => (
                        default_stops(self, self.fill.unwrap_or(Rgba::WHITE)),
                        opaque_ramp(),
                    ),
                };
                self.fill_gradient = Some(new_gradient(kind, colors, opacity, (0.0, 0.0, 1.0, 1.0)));
                self.fill.get_or_insert(Rgba::new(0x99, 0x99, 0x99, 255));
            }
            None => self.fill_gradient = None,
        }
    }
}

impl DocState {
    /// The selected paths' fill becomes `category` (Fill Options menu). A gradient keeps its
    /// stops if it already was one and gets default handles for the object.
    pub fn set_fill_category(&mut self, category: &str, opts: &ToolOptions) {
        let kind = GradientKind::from_category(category);
        let refs = self.selection.clone();
        self.edit("Fill Options", |s| {
            for r in refs {
                let bounds = s
                    .element(r)
                    .and_then(Element::bounds)
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let Some(e) = s.element_mut(r) else { continue };
                // Text starts from its own colour.
                let color = match &e.kind {
                    ElementKind::Text(t) => t.runs.first().map_or(t.color, |r| r.color),
                    _ => opts.fill.unwrap_or(Rgba::new(0x99, 0x99, 0x99, 255)),
                };
                let Some(slot) = e.fill_slot_mut() else { continue };
                let f = slot.get_or_insert_with(|| Fill {
                    color,
                    category: "fc_Solid".into(),
                    anti_alias: opts.fill_anti_alias,
                    feather: opts.fill_feather,
                    pattern: None,
                    dither: None,
                    dither_transparent: false,
                    gradient: None,
                    texture: opts.fill_texture.clone(),
                    texture_amount: 0,
                    texture_transparent: false,
                });
                f.category = category.to_owned();
                f.pattern = (category == PATTERN)
                    .then(|| {
                        let p = f.pattern.take().or_else(|| opts.fill_pattern.clone())?;
                        Some(new_pattern(&p.name, p.image, bounds))
                    })
                    .flatten();
                f.dither = (category == WEB_DITHER).then(|| dither_tile(f.color, f.dither_transparent));
                f.dither_transparent &= f.dither.is_some();
                f.gradient = kind.map(|kind| {
                    let (colors, opacity) = match &f.gradient {
                        Some(g) => (g.colors.clone(), g.opacity.clone()),
                        None => (default_stops(opts, f.color), opaque_ramp()),
                    };
                    new_gradient(kind, colors, opacity, bounds)
                });
                if let ElementKind::Text(t) = &mut e.kind {
                    crate::text::restyle(t);
                }
            }
        });
    }

    /// The first selected gradient, if any.
    pub fn selected_gradient(&self) -> Option<(ElementRef, &Gradient)> {
        self.selection.iter().find_map(|r| {
            let g = self.element(*r)?.fill_slot()??.gradient.as_ref()?;
            Some((*r, g))
        })
    }

    /// The first selected path's or text's fill: `None` with neither selected, `Some(None)` for
    /// no fill (or plain-coloured text).
    pub fn selected_fill(&self) -> Option<Option<&Fill>> {
        self.selection.iter().find_map(|r| self.element(*r)?.fill_slot())
    }

    /// The handles of the selected gradient or pattern.
    pub fn selected_fill_handles(&self) -> Option<FillHandles> {
        let f = self.selected_fill()??;
        if let Some(g) = &f.gradient {
            return Some(FillHandles {
                start: g.start,
                end: g.end,
                side: g.side,
                uses_side: g.uses_side(),
            });
        }
        f.pattern.as_ref().map(|p| FillHandles {
            start: p.start,
            end: p.end,
            side: p.side,
            uses_side: true,
        })
    }

    /// The selected fill's angle in degrees: of the line to its end handle, anticlockwise.
    pub fn fill_angle(&self) -> Option<f64> {
        let h = self.selected_fill_handles()?;
        Some(
            (h.start.1 - h.end.1)
                .atan2(h.end.0 - h.start.0)
                .to_degrees()
                .rem_euclid(360.0),
        )
    }

    /// Turn the selected fill about its round handle to `degrees`.
    pub fn set_fill_angle(&mut self, degrees: f64, live: bool) {
        let (Some(h), Some(now)) = (self.selected_fill_handles(), self.fill_angle()) else {
            return;
        };
        let (sin, cos) = (now - degrees).to_radians().sin_cos();
        let (ox, oy) = h.start;
        let turn = |p: (f64, f64)| {
            let (x, y) = (p.0 - ox, p.1 - oy);
            (ox + x * cos - y * sin, oy + x * sin + y * cos)
        };
        self.set_fill_handles(live, h.start, turn(h.end), turn(h.side));
    }

    /// The first selected path's or text object's fill and stroke, for the Tools panel; plain
    /// text has a solid fill in its first character's colour. `None` with neither selected.
    pub fn selected_attributes(&self) -> Option<(Option<Fill>, Option<firecraft_doc::Stroke>)> {
        self.selection.iter().find_map(|r| {
            let e = self.element(*r)?;
            let fill = match &e.kind {
                ElementKind::Text(t) if t.fill.is_none() => {
                    Some(solid_fill(t.runs.first().map_or(t.color, |r| r.color)))
                }
                _ => e.fill_slot()?.cloned(),
            };
            Some((fill, e.stroke_slot()?.cloned()))
        })
    }

    /// Move the selected fills' handles (`live`: part of a live edit already begun).
    pub fn set_fill_handles(&mut self, live: bool, start: (f64, f64), end: (f64, f64), side: (f64, f64)) {
        self.edit_fill("Move Fill Handle", live, move |f| {
            if let [a, b, c] = f.handles_mut().as_mut_slice() {
                (**a, **b, **c) = (start, end, side);
            }
        });
    }

    /// The selected Pattern fills use another pattern, placed afresh on each object (Fireworks
    /// MX resets the handles).
    pub fn set_fill_pattern(&mut self, name: &str, image: &RgbaImage) {
        let refs = self.selection.clone();
        self.edit("Fill Options", |s| {
            for r in refs {
                let bounds = s
                    .element(r)
                    .and_then(Element::bounds)
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let Some(e) = s.element_mut(r) else { continue };
                if let Some(Some(f)) = e.fill_slot_mut()
                    && let Some(q) = f.pattern.as_mut()
                {
                    *q = new_pattern(name, image.clone(), bounds);
                }
                if let ElementKind::Text(t) = &mut e.kind {
                    crate::text::restyle(t);
                }
            }
        });
    }

    /// The selected Web Dither fills' colours (2 x 2, by rows). The source colour becomes their
    /// average (Fireworks MX).
    pub fn set_web_dither(&mut self, tile: [Rgba; 4]) {
        self.edit_fill("Fill Options", false, |f| {
            if f.dither.is_some() {
                f.dither = Some(tile);
                let mean = |a: u8, b: u8| ((u16::from(a) + u16::from(b)).div_ceil(2)) as u8;
                f.color = Rgba::new(
                    mean(tile[0].r, tile[1].r),
                    mean(tile[0].g, tile[1].g),
                    mean(tile[0].b, tile[1].b),
                    255,
                );
            }
        });
    }

    /// The selected Web Dither fills' Transparent option: on, the tile is the websafe colour
    /// nearest the fill's, with every other pixel left empty (Fireworks MX).
    pub fn set_web_dither_transparent(&mut self, on: bool) {
        self.edit_fill("Fill Options", false, |f| {
            f.dither_transparent = on;
            f.dither = Some(dither_tile(f.color, on));
        });
    }

    /// Change the selected paths' gradients (`live`: part of a live edit already begun).
    pub fn edit_gradient(&mut self, label: &str, live: bool, f: impl Fn(&mut Gradient)) {
        self.edit_fill(label, live, |fill| {
            if let Some(g) = &mut fill.gradient {
                f(g);
            }
        });
    }

    /// The selected paths' fill edge (`live`: part of a live edit already begun).
    pub fn set_fill_edge(&mut self, anti_alias: bool, feather: u16, live: bool) {
        self.edit_fill("Fill Edge", live, move |f| {
            f.anti_alias = anti_alias;
            f.feather = feather;
        });
    }

    /// Gradient tool, dragged from `a` to `b` on a path: that path gets the tool's gradient from
    /// `a` towards `b`. Whether a path took it.
    pub fn gradient_drag(&mut self, a: (f64, f64), b: (f64, f64), opts: &ToolOptions) -> bool {
        let hit = self
            .hit_test(a.0, a.1)
            .filter(|r| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Path(_))));
        let selected = self
            .selection
            .iter()
            .copied()
            .find(|r| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Path(_))));
        let Some(target) = selected.or(hit) else {
            return false;
        };
        let tool = &opts.gradient_tool;
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let side = match tool.kind {
            GradientKind::Linear => (a.0 - dy, a.1 + dx),
            _ => (a.0 + dy, a.1 - dx),
        };
        let gradient = Gradient {
            start: a,
            end: b,
            side,
            ..tool.clone()
        };
        let color = opts.fill.unwrap_or(Rgba::WHITE);
        let category = opts.gradient_category.clone();
        self.selection = vec![target];
        self.pixels = None;
        self.edit("Gradient", |s| {
            if let Some(Element {
                kind: ElementKind::Path(p),
                ..
            }) = s.element_mut(target)
            {
                let f = p.fill.get_or_insert_with(|| Fill {
                    color,
                    category: category.clone(),
                    anti_alias: true,
                    feather: 0,
                    pattern: None,
                    dither: None,
                    dither_transparent: false,
                    gradient: None,
                    texture: None,
                    texture_amount: 0,
                    texture_transparent: false,
                });
                f.category = category;
                f.gradient = Some(gradient);
                f.pattern = None;
                f.dither = None;
                f.dither_transparent = false;
            }
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::Document;

    /// Rectangle at 60, 80, 240 x 120, as read back from Fireworks MX.
    const A: (f64, f64, f64, f64) = (60.0, 80.0, 240.0, 120.0);
    /// Narrow: 310, 20, 80 x 260.
    const B: (f64, f64, f64, f64) = (310.0, 20.0, 80.0, 260.0);

    #[test]
    fn default_handles_match_fireworks_mx() {
        use GradientKind::*;
        for (kind, bounds, want) in [
            (Linear, A, ((60.0, 140.0), (300.0, 140.0), (60.0, 380.0))),
            (Radial, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 80.0))),
            (Elliptical, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 80.0))),
            (Rectangular, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 80.0))),
            (Cone, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 80.0))),
            (Ripples, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 80.0))),
            (Pinch, A, ((180.0, 140.0), (240.0, 170.0), (240.0, 110.0))),
            (Bars, A, ((180.0, 140.0), (300.0, 140.0), (180.0, 260.0))),
            (Waves, A, ((180.0, 140.0), (240.0, 170.0), (180.0, 80.0))),
            (Satin, A, ((180.0, 140.0), (300.0, 200.0), (180.0, 80.0))),
            (Folds, A, ((180.0, 140.0), (300.0, 200.0), (180.0, 80.0))),
            (Linear, B, ((310.0, 150.0), (390.0, 150.0), (310.0, 230.0))),
            (Bars, B, ((350.0, 150.0), (390.0, 150.0), (350.0, 190.0))),
            (Pinch, B, ((350.0, 150.0), (370.0, 215.0), (370.0, 85.0))),
            (Waves, B, ((350.0, 150.0), (370.0, 215.0), (350.0, 20.0))),
        ] {
            assert_eq!(default_handles(kind, bounds), want, "{kind:?}");
        }
    }

    fn doc_with_rect() -> (DocState, ToolOptions) {
        let mut d = DocState::new(Document::new(400, 300, 72.0, None));
        let o = ToolOptions {
            stroke: Some(Rgba::new(0, 255, 0, 255)),
            fill: Some(Rgba::new(255, 0, 0, 255)),
            ..ToolOptions::default()
        };
        d.add_shape((60.0, 80.0, 300.0, 200.0), false, &o);
        (d, o)
    }

    #[test]
    fn a_first_gradient_runs_from_the_stroke_colour_to_the_fill_colour() {
        let (mut d, o) = doc_with_rect();
        d.set_fill_category("fc_Linear", &o);
        let (_, g) = d.selected_gradient().expect("a gradient");
        assert_eq!(
            g.colors,
            [(0.0, Rgba::new(0, 255, 0, 255)), (1.0, Rgba::new(255, 0, 0, 255))]
        );
        assert!(
            (g.start.0 - 60.0).abs() < 1e-6 && (g.start.1 - 140.0).abs() < 1e-6,
            "{:?}",
            g.start
        );
        // Another category keeps the stops, with its own default handles.
        d.set_fill_category("fc_Circular", &o);
        let (_, g) = d.selected_gradient().expect("a gradient");
        assert_eq!(g.colors.len(), 2);
        assert!(
            (g.start.0 - 180.0).abs() < 1e-6 && (g.start.1 - 140.0).abs() < 1e-6,
            "{:?}",
            g.start
        );
        d.set_fill_category("fc_Solid", &o);
        assert!(d.selected_gradient().is_none());
    }

    #[test]
    fn saved_gradients_carry_what_fireworks_mx_needs() {
        // MX ignores fill handles without PCFb, and uses a rectangle primitive's own copy.
        let (mut d, o) = doc_with_rect();
        d.set_fill_category("fc_Linear", &o);
        d.edit_gradient("Edit Gradient", false, |g| {
            g.opacity = vec![(0.0, 255), (1.0, 40)]
        });
        d.set_fill_edge(true, 14, false);
        let saved = firecraft_io::open(&firecraft_io::save_fireworks_png(&d.doc).expect("saves"), None)
            .expect("opens");
        let el = &saved.layers[0].elements(0)[0];
        let raw = el.raw.as_ref().expect("a stored record");
        assert_eq!(raw.child("PCF").map(|n| n.text.as_str()), Some("1"));
        let pri = raw.child("PRI").expect("a rectangle primitive");
        assert_eq!(pri.child("PCF").map(|n| n.text.as_str()), Some("1"));
        let near = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-3);
        assert!(near(pri.float_of("PSX"), 60.0) && near(pri.float_of("PEX"), 300.0));
        let ElementKind::Path(p) = &el.kind else {
            panic!("a path")
        };
        let f = p.fill.as_ref().expect("a fill");
        assert_eq!((f.category.as_str(), f.feather), ("fc_Linear", 14));
        let g = f.gradient.as_ref().expect("a gradient");
        assert_eq!(g.opacity.last().map(|s| s.1), Some(40));
        assert_eq!(g.colors.len(), 2);
    }

    #[test]
    fn web_dither_matches_fireworks_mx() {
        // (source, first, second), read back from Fireworks MX.
        for (source, first, second) in [
            (0x123456, 0x003333, 0x333366),
            (0x808080, 0x666666, 0x999999),
            (0x7f7f7f, 0x666666, 0x999999),
            (0xff8040, 0xff6633, 0xff9966),
            (0xccff33, 0xccff33, 0xccff33),
            (0xa0b0c0, 0x9999cc, 0x99cccc),
            (0x19334c, 0x003333, 0x333366),
            (0x20e080, 0x00cc66, 0x33ff99),
            (0x99cc66, 0x99cc66, 0x99cc66),
            (0x2d5a87, 0x336666, 0x336699),
            (0x0c263f, 0x000033, 0x003333),
        ] {
            let rgb = |c: u32| Rgba::new((c >> 16) as u8, (c >> 8) as u8, c as u8, 255);
            let tile = web_dither_tile(rgb(source));
            assert_eq!(
                tile,
                [rgb(first), rgb(second), rgb(second), rgb(first)],
                "{source:06x}"
            );
        }
        // Transparent: one websafe colour.
        assert_eq!(
            dither_tile(Rgba::new(0x0c, 0x26, 0x3f, 255), true),
            [Rgba::new(0, 0x33, 0x33, 255); 4]
        );
    }

    #[test]
    fn choosing_another_pattern_resets_the_handles_to_its_size() {
        // Fireworks MX: handles at 210,160 became 180,140 436,140 180,-116 for a 256 x 256 tile.
        let (mut d, mut o) = doc_with_rect();
        o.fill_pattern = Some(new_pattern(
            "A",
            RgbaImage::new(128, 128, Rgba::WHITE),
            (0.0, 0.0, 1.0, 1.0),
        ));
        d.set_fill_category(PATTERN, &o);
        d.set_fill_handles(false, (210.0, 160.0), (338.0, 160.0), (210.0, 32.0));
        d.set_fill_pattern("B", &RgbaImage::new(256, 256, Rgba::WHITE));
        let h = d.selected_fill_handles().expect("handles");
        assert_eq!(
            (h.start, h.end, h.side),
            ((180.0, 140.0), (436.0, 140.0), (180.0, -116.0))
        );
    }

    #[test]
    fn new_objects_take_on_the_last_selected_objects_fill_and_stroke() {
        // Fireworks MX: a rectangle drawn after deselecting matched the object in fill kind,
        // pattern, stroke colour and size, or had no stroke after an object without one.
        let (mut d, mut o) = doc_with_rect();
        o.fill_pattern = Some(new_pattern(
            "A",
            RgbaImage::new(8, 8, Rgba::WHITE),
            (0.0, 0.0, 1.0, 1.0),
        ));
        d.set_fill_category(PATTERN, &o);
        let (fill, stroke) = d.selected_attributes().expect("a path");
        let mut tools = ToolOptions::default();
        tools.adopt_fill(fill.as_ref());
        tools.adopt_stroke(stroke.as_ref());
        let new = tools.new_fill((0.0, 0.0, 16.0, 16.0)).expect("a fill");
        assert_eq!(new.category, PATTERN);
        let q = new.pattern.expect("a pattern");
        assert_eq!((q.name.as_str(), q.start), ("A", (8.0, 8.0)));
        assert_eq!(tools.stroke, Some(Rgba::new(0, 255, 0, 255)));
        tools.adopt_stroke(None);
        assert_eq!(tools.stroke, None);
    }

    #[test]
    fn the_preset_name_lasts_until_the_stops_change() {
        let (name, stops) = GRADIENT_PRESETS[4];
        let mut g = new_gradient(
            GradientKind::Linear,
            preset_colors(stops),
            opaque_ramp(),
            (0.0, 0.0, 9.0, 9.0),
        );
        assert_eq!(preset_name(&g), Some(name));
        g.colors.insert(1, (0.47, Rgba::new(1, 2, 3, 255)));
        assert_eq!(preset_name(&g), None);
    }

    #[test]
    fn the_fill_angle_turns_about_the_round_handle() {
        let (mut d, o) = doc_with_rect();
        d.set_fill_category("fc_Linear", &o);
        d.set_fill_handles(false, (100.0, 100.0), (200.0, 100.0), (100.0, 0.0));
        assert!(d.fill_angle().is_some_and(|a| a.abs() < 1e-6));
        d.set_fill_angle(90.0, false);
        let h = d.selected_fill_handles().expect("handles");
        assert!(
            (h.end.0 - 100.0).abs() < 1e-6 && (h.end.1 - 0.0).abs() < 1e-6,
            "{:?}",
            h.end
        );
    }

    #[test]
    fn an_enlarged_perspective_symbol_of_paths_stays_sharp() {
        use firecraft_doc::{Layer, Symbol, SymbolKind};
        use firecraft_raster::transform::Affine;
        let (mut d, _) = doc_with_rect();
        let mut layer = Layer::new("L");
        let mut path = d.doc.layers[0].elements(0)[0].clone();
        path.translate(-60.0, -80.0);
        if let Some(e) = layer.elements_mut(0) {
            e.push(path);
        }
        // The rectangle is 240 x 120; the instance shows it about 6x larger, its edge on canvas.
        d.doc.symbols = std::sync::Arc::new(vec![Symbol {
            id: 7,
            name: "S".into(),
            kind: SymbolKind::Graphic,
            layers: vec![layer],
            frames: 1,
            nine: None,
            raw: None,
        }]);
        let matrix = Affine {
            a: 6.0,
            d: 6.0,
            e: 20.5,
            f: -500.0,
            g: 1e-6,
            ..Affine::IDENTITY
        };
        if let Some(e) = d.doc.layers[0].elements_mut(0) {
            e[0] = Element::new(ElementKind::Instance {
                symbol: 7,
                name: "S".into(),
                matrix,
                bounds: Some((0.0, 0.0, 400.0, 300.0)),
            });
        }
        let image = d.doc.render(0);
        let soft = (0..120i64)
            .filter_map(|x| image.get(x, 100))
            .filter(|p| p.a != 0 && p.a != 255)
            .count();
        assert!(
            soft <= 3,
            "edge {soft} pixels wide (a magnified symbol would be about 6)"
        );
    }

    #[test]
    fn perspective_setting_survives_a_save_and_fireworks_files_keep_theirs() {
        let (mut d, _) = doc_with_rect();
        d.doc.mx_perspective = false;
        let warp = firecraft_io::save_fireworks_png(&d.doc).expect("saves");
        assert!(!firecraft_io::open(&warp, None).expect("opens").mx_perspective);
        // No FireCraft marker, as in a file Fireworks saved.
        d.doc.mx_perspective = true;
        let mx = firecraft_io::save_fireworks_png(&d.doc).expect("saves");
        assert!(firecraft_io::open(&mx, None).expect("opens").mx_perspective);
    }

    /// Fireworks MX re-rolls random strokes each time a file opens; FireCraft keeps them put.
    #[test]
    fn random_strokes_are_the_same_after_moving_and_reopening() {
        let preset = firecraft_doc::brush_presets::BRUSH_PRESETS
            .iter()
            .position(|p| p.name == "bn_Confetti")
            .expect("the Confetti preset");
        let mut d = DocState::new(Document::new(400, 300, 72.0, None));
        let o = ToolOptions {
            stroke: Some(Rgba::BLACK),
            fill: None,
            stroke_size: 30,
            stroke_preset: preset,
            ..ToolOptions::default()
        };
        d.add_shape((60.0, 60.0, 340.0, 240.0), false, &o);
        let first = d.doc.render(0);
        assert_eq!(first, d.doc.render(0));
        let path = &mut d.doc.layers[0].elements_mut(0).expect("the layer")[0];
        path.translate(0.0, -40.0);
        path.translate(0.0, 40.0);
        assert_eq!(first, d.doc.render(0), "moved up and back");
        let saved = firecraft_io::save_fireworks_png(&d.doc).expect("saves");
        assert_eq!(
            first,
            firecraft_io::open(&saved, None).expect("opens").render(0),
            "reopened"
        );
    }

    #[test]
    fn a_perspective_symbol_is_hit_on_its_shape_and_follows_edits() {
        use firecraft_doc::{Layer, Symbol, SymbolKind};
        use firecraft_raster::transform::Affine;
        let (mut d, _) = doc_with_rect();
        let mut layer = Layer::new("L");
        let path = d.doc.layers[0].elements(0)[0].clone();
        if let Some(e) = layer.elements_mut(0) {
            e.push(path);
        }
        let symbol = |layer: Layer| Symbol {
            id: 7,
            name: "S".into(),
            kind: SymbolKind::Graphic,
            layers: vec![layer],
            frames: 1,
            nine: None,
            raw: None,
        };
        d.doc.symbols = std::sync::Arc::new(vec![symbol(layer.clone())]);
        let matrix = Affine {
            g: 0.0001,
            ..Affine::IDENTITY
        };
        if let Some(e) = d.doc.layers[0].elements_mut(0) {
            e[0] = Element::new(ElementKind::Instance {
                symbol: 7,
                name: "S".into(),
                matrix,
                bounds: Some((60.0, 80.0, 240.0, 120.0)),
            });
        }
        assert!(d.hit_test(150.0, 130.0).is_some(), "on the shape");
        assert!(d.hit_test(10.0, 10.0).is_none(), "off it");
        // Moving the instance, and then editing its symbol, are both seen.
        d.doc.layers[0].elements_mut(0).expect("layer")[0].translate(100.0, 0.0);
        assert!(d.hit_test(250.0, 130.0).is_some(), "moved");
        if let Some(e) = layer.elements_mut(0) {
            e.clear();
        }
        d.doc.symbols = std::sync::Arc::new(vec![symbol(layer)]);
        assert!(d.hit_test(250.0, 130.0).is_none(), "symbol emptied");
    }

    #[test]
    fn the_mx_perspective_preference_changes_only_how_the_symbol_is_drawn() {
        use firecraft_doc::{Layer, Symbol, SymbolKind};
        use firecraft_raster::transform::Affine;
        let (mut d, o) = doc_with_rect();
        d.set_fill_category("fc_Ripple", &o);
        let mut layer = Layer::new("L");
        let path = d.doc.layers[0].elements(0)[0].clone();
        if let Some(e) = layer.elements_mut(0) {
            e.push(path);
        }
        d.doc.symbols = std::sync::Arc::new(vec![Symbol {
            id: 7,
            name: "S".into(),
            kind: SymbolKind::Graphic,
            layers: vec![layer],
            frames: 1,
            nine: None,
            raw: None,
        }]);
        let matrix = Affine {
            g: 0.002,
            ..Affine::IDENTITY
        };
        if let Some(e) = d.doc.layers[0].elements_mut(0) {
            e[0] = Element::new(ElementKind::Instance {
                symbol: 7,
                name: "S".into(),
                matrix,
                bounds: Some((0.0, 0.0, 400.0, 300.0)),
            });
        }
        let warped = d.doc.render(0);
        d.doc.mx_perspective = true;
        assert_ne!(
            warped,
            d.doc.render(0),
            "mapping the handles draws different rings"
        );
        d.doc.mx_perspective = false;
        assert_eq!(warped, d.doc.render(0));
    }

    /// Fireworks MX lays fill textures inside a mapped perspective symbol on the canvas: every
    /// tile of mechscene2's floor shows its stripes in one phase, whatever each tile's own origin.
    #[test]
    fn textures_in_a_mapped_perspective_symbol_share_the_canvas_phase() {
        use firecraft_doc::{Layer, Symbol, SymbolKind, Texture};
        use firecraft_raster::transform::Affine;
        let mut d = DocState::new(Document::new(320, 120, 72.0, None));
        let o = ToolOptions {
            stroke: None,
            fill: Some(Rgba::new(100, 100, 100, 255)),
            ..ToolOptions::default()
        };
        d.add_shape((20.0, 20.0, 120.0, 100.0), false, &o);
        d.add_shape((200.0, 20.0, 300.0, 100.0), false, &o);
        let stripes = Texture {
            name: "stripes".into(),
            width: 1,
            height: 3,
            mask: std::sync::Arc::new(vec![255, 0, 255]),
        };
        let mut layer = Layer::new("L");
        for (i, e) in d.doc.layers[0].elements(0).iter().enumerate() {
            let mut e = e.clone();
            if let ElementKind::Path(p) = &mut e.kind {
                // Origins one row apart: drawn apart, their stripes would be out of phase.
                p.texture_origin = Some((0.0, i as f64));
                let f = p.fill.as_mut().expect("a fill");
                f.texture = Some(stripes.clone());
                f.texture_amount = 1000;
            }
            if let Some(v) = layer.elements_mut(0) {
                v.push(e);
            }
        }
        d.doc.symbols = std::sync::Arc::new(vec![Symbol {
            id: 7,
            name: "S".into(),
            kind: SymbolKind::Graphic,
            layers: vec![layer],
            frames: 1,
            nine: None,
            raw: None,
        }]);
        if let Some(e) = d.doc.layers[0].elements_mut(0) {
            e.truncate(1);
            e[0] = Element::new(ElementKind::Instance {
                symbol: 7,
                name: "S".into(),
                matrix: Affine {
                    g: 0.0005,
                    ..Affine::IDENTITY
                },
                bounds: Some((20.0, 20.0, 280.0, 80.0)),
            });
        }
        d.doc.mx_perspective = true;
        let image = d.doc.render(0);
        let light_row = |x: i64| {
            (40..43)
                .max_by_key(|&y| image.get(x, y).map_or(0, |p| p.r))
                .expect("rows")
        };
        assert_eq!(light_row(60) % 3, light_row(240) % 3);
        assert_eq!(light_row(60) % 3, 1, "anchored at the canvas origin");
    }

    #[test]
    fn editing_a_dither_colour_makes_the_source_their_average() {
        // Fireworks MX: #000033 and #ffffff gave a fill colour of #808099.
        let (mut d, o) = doc_with_rect();
        d.set_fill_category(WEB_DITHER, &o);
        let (a, b) = (Rgba::new(0, 0, 0x33, 255), Rgba::WHITE);
        d.set_web_dither([a, b, b, a]);
        let f = d.selected_fill().flatten().expect("a fill");
        assert_eq!(f.color, Rgba::new(0x80, 0x80, 0x99, 255));
    }

    #[test]
    fn a_new_pattern_starts_at_the_objects_centre_one_tile_long() {
        // Fireworks MX: a 128 x 128 pattern on 60,80 240 x 120 gave 180,140 308,140 180,12.
        let p = new_pattern(
            "Berber Rug",
            RgbaImage::new(128, 128, Rgba::WHITE),
            (60.0, 80.0, 240.0, 120.0),
        );
        assert_eq!(
            (p.start, p.end, p.side),
            ((180.0, 140.0), (308.0, 140.0), (180.0, 12.0))
        );
    }

    #[test]
    fn pattern_and_web_dither_survive_a_save() {
        let (mut d, mut o) = doc_with_rect();
        o.fill_pattern = Some(new_pattern(
            "Tiny",
            RgbaImage::new(3, 2, Rgba::new(10, 20, 30, 255)),
            (0.0, 0.0, 1.0, 1.0),
        ));
        d.set_fill_category(PATTERN, &o);
        let bytes = firecraft_io::save_fireworks_png(&d.doc).expect("saves");
        let saved = firecraft_io::open(&bytes, None).expect("opens");
        let ElementKind::Path(p) = &saved.layers[0].elements(0)[0].kind else {
            panic!("a path")
        };
        let f = p.fill.as_ref().expect("a fill");
        let q = f.pattern.as_ref().expect("a pattern");
        assert_eq!(
            (q.name.as_str(), q.image.width(), q.image.height()),
            ("Tiny", 3, 2)
        );

        d.set_fill_category(WEB_DITHER, &o);
        d.set_web_dither_transparent(true);
        let bytes = firecraft_io::save_fireworks_png(&d.doc).expect("saves");
        let saved = firecraft_io::open(&bytes, None).expect("opens");
        let el = &saved.layers[0].elements(0)[0];
        let ElementKind::Path(p) = &el.kind else {
            panic!("a path")
        };
        let f = p.fill.as_ref().expect("a fill");
        assert!(f.pattern.is_none() && f.dither.is_some() && f.dither_transparent);
        assert_eq!(f.category, WEB_DITHER);
    }

    #[test]
    fn presets_are_sorted_and_within_range() {
        for (name, stops) in GRADIENT_PRESETS {
            assert!(stops.windows(2).all(|w| w[0].0 < w[1].0), "{name}");
            assert!(stops.iter().all(|s| (0.0..=1.0).contains(&s.0)), "{name}");
        }
    }

    #[test]
    fn a_new_shape_takes_the_tool_gradient_with_its_own_handles() {
        let mut d = DocState::new(Document::new(400, 300, 72.0, None));
        let mut o = ToolOptions {
            fill: Some(Rgba::new(255, 0, 0, 255)),
            ..ToolOptions::default()
        };
        o.set_fill_category("fc_Linear");
        d.add_shape((60.0, 80.0, 300.0, 200.0), false, &o);
        let (_, g) = d.selected_gradient().expect("a gradient");
        assert!(
            (g.start.0 - 60.0).abs() < 1e-6 && (g.end.0 - 300.0).abs() < 1e-6,
            "{g:?}"
        );
    }
}
