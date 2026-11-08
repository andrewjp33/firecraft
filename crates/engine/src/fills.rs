//! Fill categories, gradients and fill edges: Fireworks MX's defaults (measured by reading each
//! fill back through its scripting API; docs/behavior/gradients.md) and the commands that edit them.

use firecraft_doc::{Element, ElementKind, Fill};
use firecraft_raster::Rgba;
use firecraft_raster::gradient::{Gradient, GradientKind};

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
    GRADIENT_FILLS
        .iter()
        .find(|(c, _)| *c == category)
        .map_or("Solid", |(_, l)| l)
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
        Some(Fill {
            color,
            category: self.fill_category.clone(),
            anti_alias: self.fill_anti_alias,
            feather: self.fill_feather,
            gradient,
            texture: self.fill_texture.clone(),
            texture_amount: 0,
            texture_transparent: false,
        })
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
                let Some(Element {
                    kind: ElementKind::Path(p),
                    ..
                }) = s.element_mut(r)
                else {
                    continue;
                };
                let f = p.fill.get_or_insert_with(|| Fill {
                    color: opts.fill.unwrap_or(Rgba::new(0x99, 0x99, 0x99, 255)),
                    category: "fc_Solid".into(),
                    anti_alias: opts.fill_anti_alias,
                    feather: opts.fill_feather,
                    gradient: None,
                    texture: opts.fill_texture.clone(),
                    texture_amount: 0,
                    texture_transparent: false,
                });
                f.category = category.to_owned();
                f.gradient = kind.map(|kind| {
                    let (colors, opacity) = match &f.gradient {
                        Some(g) => (g.colors.clone(), g.opacity.clone()),
                        None => (default_stops(opts, f.color), opaque_ramp()),
                    };
                    new_gradient(kind, colors, opacity, bounds)
                });
            }
        });
    }

    /// The first selected gradient, if any.
    pub fn selected_gradient(&self) -> Option<(ElementRef, &Gradient)> {
        self.selection
            .iter()
            .find_map(|r| match self.element(*r).map(|e| &e.kind) {
                Some(ElementKind::Path(p)) => p.fill.as_ref()?.gradient.as_ref().map(|g| (*r, g)),
                _ => None,
            })
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
                    gradient: None,
                    texture: None,
                    texture_amount: 0,
                    texture_transparent: false,
                });
                f.category = category;
                f.gradient = Some(gradient);
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
