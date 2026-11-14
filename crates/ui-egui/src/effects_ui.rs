//! Live Effects in the Property inspector: the effects list (on/off, edit, order, delete), the
//! "+" menu of effects by category, and each effect's settings popup.
//!
//! New effects get the same parameter set Fireworks MX writes (from its own files), so they open
//! in Fireworks. Edits update the canvas live and count as one undo step per popup.

use std::collections::HashMap;

use egui::{Color32, ComboBox, Sense, vec2};
use firecraft_doc::{EffectEntry, EffectParam};
use firecraft_raster::Rgba;
use firecraft_raster::plugins::Params;

use crate::{FireCraftApp, theme};

/// The effects list's UI state.
#[derive(Clone, Debug, Default)]
pub struct EffectsUi {
    /// Highlighted row.
    pub selected: Option<usize>,
    /// The effect whose settings popup is open.
    pub editing: Option<Editing>,
    /// Levels/Curves popup state.
    pub tables: crate::tables_ui::TableEditor,
    /// The Filters menu's settings popup, while open.
    pub filter: Option<FilterDialog>,
    /// The last filter applied from the Filters menu (Repeat Filter; its popup starts from
    /// these settings next time).
    pub last_filter: Option<EffectEntry>,
}

/// A filter from the Filters menu being set up: previewed on the document (a live edit begun
/// when the popup opened), applied on OK.
#[derive(Clone, Debug)]
pub struct FilterDialog {
    doc: u64,
    entry: EffectEntry,
    /// Fireworks asks before turning objects that aren't bitmaps into bitmaps.
    confirmed: bool,
    /// The live edit (and first preview) has begun.
    started: bool,
}

/// An open settings popup: OK keeps its changes (one undo step), Cancel restores the
/// document as it was when it opened (removing the effect if it was just added).
#[derive(Clone, Debug)]
pub struct Editing {
    doc: u64,
    index: usize,
    /// The objects being edited (the popup closes, keeping its changes, if this changes).
    selection: Vec<firecraft_engine::ElementRef>,
    /// The effects list when the popup opened (before the effect was added, if it was).
    original: Vec<EffectEntry>,
    /// Opened by adding the effect from the "+" menu.
    added: bool,
}

/// How a setting is edited and stored.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Plain number text ("10", "4.5"); `log`: logarithmic slider (wide ranges of sizes, used
    /// mostly at the low end).
    Number {
        min: f64,
        max: f64,
        decimals: usize,
        log: bool,
    },
    Bool,
    /// `#rrggbb`.
    Color,
    /// `#rrggbbaa` (shadow colour and opacity).
    ColorAlpha,
    /// One of (stored value, label).
    Choice(&'static [(&'static str, &'static str)]),
    /// Plug-in values (Photoshop descriptors): number text, "N pixels" hex text, "RGB: ..." hex
    /// text, enumeration (type, [(value, label)]).
    PluginPixels {
        min: f64,
        max: f64,
    },
    PluginColor,
    PluginEnum(&'static str, &'static [(&'static str, &'static str)]),
}

struct Field {
    key: &'static str,
    label: &'static str,
    kind: Kind,
}

const fn num(key: &'static str, label: &'static str, min: f64, max: f64, decimals: usize) -> Field {
    Field {
        key,
        label,
        kind: Kind::Number {
            min,
            max,
            decimals,
            log: false,
        },
    }
}

/// A number with a logarithmic slider (radii, seeds: wide ranges used mostly at the low end).
const fn num_log(key: &'static str, label: &'static str, min: f64, max: f64, decimals: usize) -> Field {
    Field {
        key,
        label,
        kind: Kind::Number {
            min,
            max,
            decimals,
            log: true,
        },
    }
}

/// Effect settings sliders: a long track, logarithmic for wide size ranges so the low end
/// (where most effects are used) gets most of the travel.
fn fx_slider(v: &mut f64, min: f64, max: f64, log: bool) -> egui::Slider<'_> {
    egui::Slider::new(v, min..=max)
        .logarithmic(log)
        // Below this a logarithmic slider jumps to 0 (ranges that start at 0).
        .smallest_positive(if min > 0.0 { min } else { 0.5 })
}

const fn field(key: &'static str, label: &'static str, kind: Kind) -> Field {
    Field { key, label, kind }
}

struct EffectDef {
    category: &'static str,
    name: &'static str,
    moa: &'static str,
    /// Stored parameters for a new effect (after the common header).
    defaults: &'static [(&'static str, &'static str)],
    fields: &'static [Field],
    /// Filter-style effects carry `MB_filter_preview_tile_size`.
    filter: bool,
    /// Plug-in category name as Fireworks stores it.
    plugin_category: Option<&'static str>,
}

const SLOPES: &[(&str, &str)] = &[
    ("0", "Flat"),
    ("1", "Smooth"),
    ("2", "Sloped"),
    ("3", "Frame 1"),
    ("4", "Frame 2"),
    ("5", "Ring"),
    ("6", "Ruffle"),
];
const BUTTONS: &[(&str, &str)] = &[
    ("0", "Raised"),
    ("1", "Highlighted"),
    ("2", "Inset"),
    ("3", "Inverted"),
];
const BLENDS: &[(&str, &str)] = &[
    ("0", "Normal"),
    ("2", "Multiply"),
    ("5", "Screen"),
    ("9", "Darken"),
    ("10", "Lighten"),
    ("3", "Difference"),
    ("11", "Hue"),
    ("12", "Saturation"),
    ("13", "Color"),
    ("14", "Luminosity"),
    ("17", "Invert"),
    ("29", "Tint"),
    ("25", "Erase"),
];
const EDGE_MODES: &[(&str, &str)] = &[
    ("edM0", "Lumpy"),
    ("edM1", "Rough"),
    ("edM3", "Halftone Dots"),
    ("edM5", "Halftone Lines"),
    ("edM4", "Pixels"),
    ("edM6", "Torn Paper"),
];
const PLACEMENT: &[(&str, &str)] = &[("beT0", "Inside marquee"), ("beT1", "Outside marquee")];

/// Bevel/Glow presets share one parameter set (Fireworks' own defaults).
macro_rules! bevel_defaults {
    ($ty:literal, $width:literal, $contrast:literal, $soft:literal, $glow:literal, $mask:literal) => {
        &[
            ("BevelType", $ty),
            ("BevelWidth", $width),
            ("BevelContrast", $contrast),
            ("AngleSoftness", $soft),
            ("EdgeThreshold", "0"),
            ("ShadowColor", "#000000"),
            ("ShowObject", "false"),
            ("LightAngle", "135"),
            ("HitBlendColor", "#ffffff3f"),
            ("ButtonState", "0"),
            ("HiliteColor", "#ffffff"),
            ("EmbossFaceColor", "#00000000"),
            ("GlowStartDistance", "0"),
            ("DownBlendColor", "#0000003f"),
            ("GlowWidth", $glow),
            ("SlopeMultiplier", "1"),
            ("MaskSoftness", $mask),
            ("LightDistance", "100"),
            ("SlopeType", "0"),
            ("OuterBevelColor", "#df0000"),
        ]
    };
}

const BEVEL_FIELDS: &[Field] = &[
    field("SlopeType", "Edge shape", Kind::Choice(SLOPES)),
    num("BevelWidth", "Width", 0.0, 100.0, 0),
    num("BevelContrast", "Contrast", 0.0, 100.0, 0),
    num("AngleSoftness", "Softness", 0.0, 10.0, 0),
    num("LightAngle", "Angle", 0.0, 360.0, 0),
    field("ButtonState", "Button preset", Kind::Choice(BUTTONS)),
];
const OUTER_BEVEL_FIELDS: &[Field] = &[
    field("SlopeType", "Edge shape", Kind::Choice(SLOPES)),
    field("OuterBevelColor", "Color", Kind::Color),
    num("BevelWidth", "Width", 0.0, 100.0, 0),
    num("BevelContrast", "Contrast", 0.0, 100.0, 0),
    num("AngleSoftness", "Softness", 0.0, 10.0, 0),
    num("LightAngle", "Angle", 0.0, 360.0, 0),
    field("ButtonState", "Button preset", Kind::Choice(BUTTONS)),
];
const EMBOSS_FIELDS: &[Field] = &[
    num("BevelWidth", "Width", 0.0, 100.0, 0),
    num("BevelContrast", "Contrast", 0.0, 100.0, 0),
    num("AngleSoftness", "Softness", 0.0, 10.0, 0),
    num("LightAngle", "Angle", 0.0, 360.0, 0),
    field("ShowObject", "Show object", Kind::Bool),
];
const SHADOW_FIELDS: &[Field] = &[
    num("ShadowDistance", "Distance", 0.0, 100.0, 0),
    field("ShadowColor", "Color", Kind::ColorAlpha),
    num("ShadowBlur", "Softness", 0.0, 50.0, 0),
    num("ShadowAngle", "Angle", 0.0, 360.0, 0),
];
const GLOW_FIELDS: &[Field] = &[
    num("GlowWidth", "Width", 0.0, 100.0, 0),
    field("OuterBevelColor", "Color", Kind::Color),
    num("BevelContrast", "Opacity", 0.0, 100.0, 0),
    num("MaskSoftness", "Softness", 0.0, 50.0, 0),
    num("GlowStartDistance", "Offset", 0.0, 50.0, 0),
];

/// Fireworks MX puts Inner Glow's width back to 2 when set to 0 or 1 (not Glow's or Inner Shadow's).
const INNER_GLOW_FIELDS: &[Field] = &[
    num("GlowWidth", "Width", 2.0, 100.0, 0),
    field("OuterBevelColor", "Color", Kind::Color),
    num("BevelContrast", "Opacity", 0.0, 100.0, 0),
    num("MaskSoftness", "Softness", 0.0, 50.0, 0),
    num("GlowStartDistance", "Offset", 0.0, 50.0, 0),
];

const EC: &str = " Eye Candy 4000 LE";

/// A filter with no settings (Blur, Sharpen, Find Edges...), as Fireworks MX stores it.
macro_rules! plain_filter {
    ($cat:literal, $name:literal, $moa:literal) => {
        EffectDef {
            category: $cat,
            name: $name,
            moa: $moa,
            defaults: &[],
            fields: &[],
            filter: true,
            plugin_category: None,
        }
    };
}

/// Levels' parameters for one channel at their neutral values.
macro_rules! levels_defaults {
    () => {
        &[
            ("source_low_rgb", "0"),
            ("source_high_rgb", "255"),
            ("dest_low_rgb", "0"),
            ("dest_high_rgb", "255"),
            ("gamma_rgb", "1"),
            ("source_low_red", "0"),
            ("source_high_red", "255"),
            ("dest_low_red", "0"),
            ("dest_high_red", "255"),
            ("gamma_red", "1"),
            ("source_low_green", "0"),
            ("source_high_green", "255"),
            ("dest_low_green", "0"),
            ("dest_high_green", "255"),
            ("gamma_green", "1"),
            ("source_low_blue", "0"),
            ("source_high_blue", "255"),
            ("dest_low_blue", "0"),
            ("dest_high_blue", "255"),
            ("gamma_blue", "1"),
        ]
    };
}

/// Effects edited in a popup of their own (`tables_ui`), not a list of fields.
const LEVELS: &str = "Levels...";
const CURVES: &str = "Curves...";

fn has_editor(d: &EffectDef) -> bool {
    !d.fields.is_empty() || d.name == LEVELS || d.name == CURVES
}

const DEFS: &[EffectDef] = &[
    plain_filter!("Adjust Color", "Auto Levels", "{d04ef8c1-71b4-11d1-8c8200a024cdc039}"),
    EffectDef {
        category: "Adjust Color",
        name: "Brightness/Contrast...",
        moa: "{3439b08c-1921-11d3-9bde00e02910d580}",
        defaults: &[("brightness_amount", "0"), ("contrast_amount", "0")],
        fields: &[
            num("brightness_amount", "Brightness", -100.0, 100.0, 0),
            num("contrast_amount", "Contrast", -100.0, 100.0, 0),
        ],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Adjust Color",
        name: "Color Fill",
        moa: "{dd54adc0-a279-11d3-b92a000502f3fdbe}",
        defaults: &[("Color", "#ff0000"), ("Opacity", "100"), ("Blendmode", "0")],
        fields: &[
            field("Color", "Color", Kind::Color),
            num("Opacity", "Opacity", 0.0, 100.0, 0),
            field("Blendmode", "Blend mode", Kind::Choice(BLENDS)),
        ],
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Adjust Color",
        name: CURVES,
        moa: "{3439b08e-1923-11d3-9bde00e02910d580}",
        defaults: &[],
        fields: &[],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Adjust Color",
        name: "Hue/Saturation...",
        moa: "{3439b08d-1922-11d3-9bde00e02910d580}",
        defaults: &[
            ("lightness_amount", "0"),
            ("hls_colorize", "false"),
            ("saturation_amount", "0"),
            ("hue_amount", "0"),
        ],
        fields: &[
            num("hue_amount", "Hue", -180.0, 180.0, 0),
            num("saturation_amount", "Saturation", -100.0, 100.0, 0),
            num("lightness_amount", "Lightness", -100.0, 100.0, 0),
            field("hls_colorize", "Colorize", Kind::Bool),
        ],
        filter: true,
        plugin_category: None,
    },
    // FireCraft's: stored as Hue/Saturation at saturation -100 (what Fireworks MX shows), marked
    // so FireCraft renders perceived lightness.
    EffectDef {
        category: "Adjust Color",
        name: "Grayscale",
        moa: "{3439b08d-1922-11d3-9bde00e02910d580}",
        defaults: &[
            ("lightness_amount", "0"),
            ("hls_colorize", "false"),
            ("saturation_amount", "-100"),
            ("hue_amount", "0"),
            ("firecraft_effect", "grayscale"),
        ],
        fields: &[],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Adjust Color",
        name: "Invert",
        moa: "{d2541291-70d6-11d1-8c8000a024cdc039}",
        defaults: &[],
        fields: &[],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Adjust Color",
        name: LEVELS,
        moa: "{d04ef8c2-71b5-11d1-8c8200a024cdc039}",
        defaults: levels_defaults!(),
        fields: &[],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Bevel and Emboss",
        name: "Inner Bevel",
        moa: "{7fe61102-6ce2-11d1-8c76000502701850}",
        defaults: bevel_defaults!("0", "10", "75", "3", "0", "0"),
        fields: BEVEL_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Bevel and Emboss",
        name: "Outer Bevel",
        moa: "{7fe61102-6ce2-11d1-8c76000502701850}",
        defaults: bevel_defaults!("1", "10", "75", "3", "0", "0"),
        fields: OUTER_BEVEL_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Bevel and Emboss",
        name: "Raised Emboss",
        moa: "{7fe61102-6ce2-11d1-8c76000502701850}",
        defaults: bevel_defaults!("2", "10", "75", "3", "0", "0"),
        fields: EMBOSS_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Bevel and Emboss",
        name: "Inset Emboss",
        moa: "{7fe61102-6ce2-11d1-8c76000502701850}",
        defaults: bevel_defaults!("3", "10", "75", "3", "0", "0"),
        fields: EMBOSS_FIELDS,
        filter: false,
        plugin_category: None,
    },
    plain_filter!("Blur", "Blur", "{f1cfce41-718e-11d1-8c8200a024cdc039}"),
    plain_filter!("Blur", "Blur More", "{f1cfce42-718e-11d1-8c8200a024cdc039}"),
    EffectDef {
        category: "Blur",
        name: "Gaussian Blur...",
        moa: "{d04ef8c0-71b3-11d1-8c8200a024cdc039}",
        defaults: &[("gaussian_blur_radius", "1")],
        fields: &[num_log("gaussian_blur_radius", "Radius", 0.1, 250.0, 1)],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Other",
        name: "Convert to Alpha",
        moa: "{2932d5a2-ca48-11d1-8561000502701850}",
        defaults: &[],
        fields: &[],
        filter: true,
        plugin_category: None,
    },
    plain_filter!("Other", "Find Edges", "{fc7093f1-f95c-11d0-8be200a024cdc039}"),
    EffectDef {
        category: "Shadow and Glow",
        name: "Drop Shadow",
        moa: "{a7944db8-6ce2-11d1-8c76000502701850}",
        defaults: &[
            ("ShadowColor", "#000000a5"),
            ("ShadowAngle", "315"),
            ("ShadowBlur", "4"),
            ("ShadowDistance", "7"),
            ("ShadowType", "0"),
        ],
        fields: SHADOW_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Shadow and Glow",
        name: "Inner Shadow",
        moa: "{5600f702-774c-11d3-baad0000861f4d01}",
        defaults: &[
            ("ShadowColor", "#000000a5"),
            ("ShadowAngle", "315"),
            ("ShadowBlur", "4"),
            ("ShadowDistance", "7"),
            ("ShadowType", "0"),
        ],
        fields: SHADOW_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Shadow and Glow",
        name: "Glow",
        moa: "{7fe61102-6ce2-11d1-8c76000502701850}",
        defaults: bevel_defaults!("4", "10", "65", "25", "5", "12"),
        fields: GLOW_FIELDS,
        filter: false,
        plugin_category: None,
    },
    EffectDef {
        category: "Shadow and Glow",
        name: "Inner Glow",
        moa: "{2ba87123-8220-11d3-baad0000861f4d01}",
        defaults: &[
            ("BevelContrast", "65"),
            ("GlowStartDistance", "0"),
            ("GlowWidth", "4"),
            ("MaskSoftness", "4"),
            ("OuterBevelColor", "#000000"),
        ],
        fields: INNER_GLOW_FIELDS,
        filter: false,
        plugin_category: None,
    },
    plain_filter!("Sharpen", "Sharpen", "{c20952b1-fc76-11d0-8be700a024cdc039}"),
    plain_filter!("Sharpen", "Sharpen More", "{1f2f2591-9db7-11d1-8cad00a024cdc039}"),
    EffectDef {
        category: "Sharpen",
        name: "Unsharp Mask...",
        moa: "{f1cfce44-718e-11d1-8c8200a024cdc039}",
        // FirePixel's reading of Fireworks' dialog defaults.
        defaults: &[
            ("unsharp_mask_amount", "50"),
            ("unsharp_mask_radius", "4.1"),
            ("unsharp_mask_threshold", "0"),
        ],
        fields: &[
            num("unsharp_mask_amount", "Sharpen amount (%)", 1.0, 500.0, 0),
            num_log("unsharp_mask_radius", "Pixel radius", 0.1, 250.0, 1),
            num("unsharp_mask_threshold", "Threshold", 0.0, 255.0, 0),
        ],
        filter: true,
        plugin_category: None,
    },
    EffectDef {
        category: "Eye Candy 4000 LE",
        name: "Bevel Boss...",
        moa: "{d810e821-f86b-11d0-656341346564694e}",
        defaults: &[
            ("external_filter_have_scripting_data", "true"),
            ("external_filter_key__Wdth", "31302e303020706978656c73 0"),
            ("external_filter_key__Cntr", "50"),
            ("external_filter_key__Smth", "50"),
            ("external_filter_key__bvlS", "42657469 62655430"),
            ("external_filter_key__0x466c2020", "true"),
            ("external_filter_key__Drct", "135"),
            ("external_filter_key__INcl", "45"),
            ("external_filter_key__Brgh", "100"),
            ("external_filter_key__Shrp", "25"),
            (
                "external_filter_key__0x436c7220",
                "5247423a203130302e3025203130302e3025203130302e30252028323535203235352032353529 0",
            ),
            (
                "external_filter_key__sdwC",
                "5247423a20302e302520302e302520302e30252028302030203029 0",
            ),
            (
                "external_filter_key__BEvl",
                "426576656c3a20332f302e30303020302e30303020302f302e36373120302e34353820302f312e30303020312e3030302030 0",
            ),
            ("external_filter_key__Drkn", "0"),
        ],
        fields: &[
            field("Wdth", "Bevel width", Kind::PluginPixels { min: 1.0, max: 500.0 }),
            num("Cntr", "Height scale", 0.0, 100.0, 0),
            num("Smth", "Smoothness", 0.0, 100.0, 0),
            field("bvlS", "Placement", Kind::PluginEnum("Beti", PLACEMENT)),
            field("Fl  ", "Shade interior", Kind::Bool),
            num("Drct", "Direction", 0.0, 360.0, 0),
            num("INcl", "Inclination", 0.0, 90.0, 0),
            num("Brgh", "Highlight brightness", 0.0, 100.0, 0),
            num("Shrp", "Highlight size", 0.0, 100.0, 0),
            field("Clr ", "Highlight color", Kind::PluginColor),
            field("sdwC", "Shadow color", Kind::PluginColor),
            num("Drkn", "Darken deep areas", 0.0, 100.0, 0),
        ],
        filter: true,
        plugin_category: Some(EC),
    },
    EffectDef {
        category: "Eye Candy 4000 LE",
        name: "Marble...",
        moa: "{d810e821-f86b-11d0-6563413465646d41}",
        defaults: &[
            ("external_filter_have_scripting_data", "true"),
            (
                "external_filter_key__BEco",
                "5247423a2038302e30252038302e30252038302e30252028323034203230342032303429 0",
            ),
            (
                "external_filter_key__VEcl",
                "5247423a2032302e30252032302e30252032302e30252028353120353120353129 0",
            ),
            ("external_filter_key__FEsi", "35302e303020706978656c73 0"),
            ("external_filter_key__VEcv", "50"),
            ("external_filter_key__VEth", "30"),
            ("external_filter_key__VEro", "50"),
            ("external_filter_key__RndS", "1"),
            ("external_filter_key__TIle", "false"),
        ],
        fields: &[
            field("BEco", "Bedrock color", Kind::PluginColor),
            field("VEcl", "Vein color", Kind::PluginColor),
            field("FEsi", "Vein size", Kind::PluginPixels { min: 2.0, max: 500.0 }),
            num("VEcv", "Vein coverage", 0.0, 100.0, 0),
            num("VEth", "Vein thickness", 0.0, 100.0, 0),
            num("VEro", "Vein roughness", 0.0, 100.0, 0),
            num_log("RndS", "Random seed", 1.0, 99999.0, 0),
        ],
        filter: true,
        plugin_category: Some(EC),
    },
    EffectDef {
        category: "Eye Candy 4000 LE",
        name: "Motion Trail...",
        moa: "{d810e821-f86b-11d0-6563413465646d54}",
        defaults: &[
            ("external_filter_have_scripting_data", "true"),
            ("external_filter_key__Drct", "0"),
            ("external_filter_key__Lngt", "34302e303020706978656c73 0"),
            ("external_filter_key__TApe", "40"),
            ("external_filter_key__Opct", "95"),
            ("external_filter_key__0x45646720", "true"),
            ("external_filter_key__0x466c2020", "false"),
        ],
        fields: &[
            num("Drct", "Direction", 0.0, 360.0, 0),
            field(
                "Lngt",
                "Length",
                Kind::PluginPixels {
                    min: 1.0,
                    max: 1000.0,
                },
            ),
            num("TApe", "Taper", 0.0, 100.0, 0),
            num("Opct", "Overall opacity", 0.0, 100.0, 0),
            field("Edg ", "Smear color from edges", Kind::Bool),
            field("Fl  ", "Draw only outside selection", Kind::Bool),
        ],
        filter: true,
        plugin_category: Some(EC),
    },
    EffectDef {
        category: "Alien Skin Splat LE",
        name: "Edges...",
        moa: "{d810e821-f86b-11d0-7371473173676544}",
        defaults: &[
            ("external_filter_have_scripting_data", "true"),
            ("external_filter_key__Wdth", "31322e303020706978656c73 0"),
            ("external_filter_key__BrdT", "302e303020706978656c73 0"),
            ("external_filter_key__0x53636c20", "1"),
            ("external_filter_key__Drct", "45"),
            ("external_filter_key__0x45646720", "45646d6f 65644d33"),
            (
                "external_filter_key__0x436c7220",
                "5247423a203130302e3025203130302e3025203130302e30252028323535203235352032353529 0",
            ),
            ("external_filter_key__0x466c2020", "true"),
            ("external_filter_key__RndS", "1"),
        ],
        fields: &[
            field("Edg ", "Edge mode", Kind::PluginEnum("Edmo", EDGE_MODES)),
            field("Wdth", "Edge width", Kind::PluginPixels { min: 1.0, max: 500.0 }),
            field("BrdT", "Margin", Kind::PluginPixels { min: 0.0, max: 200.0 }),
            num("Scl ", "Feature size", 0.2, 10.0, 1),
            num("Drct", "Direction", 0.0, 360.0, 0),
            field("Clr ", "Color", Kind::PluginColor),
            field("Fl  ", "Transparent fill", Kind::Bool),
            num_log("RndS", "Random seed", 1.0, 99999.0, 0),
        ],
        filter: true,
        plugin_category: Some("Alien Skin Splat LE"),
    },
];

/// The definition matching an entry (by MoaID, and BevelType for the bevel/glow family).
fn def_for(e: &EffectEntry) -> Option<&'static EffectDef> {
    let bevel = e.get("BevelType");
    // A FireCraft effect stored as a Fireworks one: matched while it still is one (edited in
    // Fireworks MX, it's that Fireworks effect even though the marker stays).
    let own = matches!(e.effect(), Some(firecraft_raster::effects::Effect::Grayscale)).then_some("grayscale");
    DEFS.iter().find(|d| {
        let marker = d.defaults.iter().find(|(k, _)| *k == "firecraft_effect").map(|(_, v)| *v);
        d.moa == e.moa_id()
            && marker == own
            && match bevel {
                Some(t) => d.defaults.iter().any(|(k, v)| *k == "BevelType" && *v == t),
                None => true,
            }
    })
}

/// The effects the "+" menu offers, as (category, name).
pub fn catalogue() -> impl Iterator<Item = (&'static str, &'static str)> {
    DEFS.iter().map(|d| (d.category, d.name))
}

/// A new effect by its menu name, with Fireworks' default parameters.
pub fn new_effect(name: &str) -> Option<EffectEntry> {
    DEFS.iter().find(|d| d.name == name).map(create)
}

/// A new effect with Fireworks' default parameters.
fn create(d: &EffectDef) -> EffectEntry {
    let mut pairs: Vec<(&str, &str)> = vec![("EffectIsVisible", "true"), ("EffectMoaID", d.moa)];
    if let Some(cat) = d.plugin_category {
        pairs.push(("category", cat));
    }
    pairs.push(("name", d.name));
    if d.filter {
        pairs.push(("MB_filter_preview_tile_size", "-1 -1"));
    }
    pairs.push(("mkbFile_WriteOnly_TemporaryEffectUiName", d.name));
    pairs.extend_from_slice(d.defaults);
    let mut e = EffectEntry::from_pairs(&pairs);
    if d.name == CURVES {
        crate::tables_ui::init_curves(&mut e);
    }
    e
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// Storage key: plug-in parameters are `external_filter_key__` + code (hex when not plain).
fn key_for(d: &EffectDef, f: &Field) -> String {
    if d.plugin_category.is_none() {
        return f.key.to_owned();
    }
    if f.key.len() == 4 && f.key.bytes().all(|b| b.is_ascii_alphanumeric()) {
        format!("external_filter_key__{}", f.key)
    } else {
        format!("external_filter_key__0x{}", hex(f.key))
    }
}

/// One setting's widget; true when it changed (the entry is updated).
fn edit_field(ui: &mut egui::Ui, d: &EffectDef, f: &Field, e: &mut EffectEntry) -> bool {
    let key = key_for(d, f);
    let text: HashMap<String, String> = e
        .params
        .iter()
        .filter_map(|(k, v)| match v {
            EffectParam::Text(t) => Some((k.clone(), t.clone())),
            EffectParam::Node(_) => None,
        })
        .collect();
    let plugin = Params(&text);
    let raw = e.get(&key).unwrap_or("").to_owned();
    ui.label(f.label);
    let mut out: Option<String> = None;
    match f.kind {
        Kind::Number {
            min,
            max,
            decimals,
            log,
        } => {
            let mut v: f64 = raw.trim().parse().unwrap_or(min.max(0.0).min(max));
            let r = ui.add(
                fx_slider(&mut v, min, max, log)
                    .max_decimals(decimals)
                    .min_decimals(0),
            );
            if r.changed() {
                out = Some(if decimals == 0 {
                    format!("{}", v.round() as i64)
                } else {
                    format!("{:.*}", decimals, v)
                });
            }
        }
        Kind::Bool => {
            let mut v = raw.trim() == "true";
            if ui.checkbox(&mut v, "").changed() {
                out = Some(if v { "true" } else { "false" }.to_owned());
            }
        }
        Kind::Color | Kind::ColorAlpha => {
            // Fireworks CS's colour pop-up (FirePixel's re-creation).
            use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
            let c = Rgba::from_hex(&raw).unwrap_or(Rgba::BLACK);
            let alpha = matches!(f.kind, Kind::ColorAlpha);
            // The colour itself (the shadow's opacity is its own field).
            let shown = Rgba { a: 255, ..c };
            let mut arr = [c.r, c.g, c.b, c.a];
            let mut changed = false;
            if let Some(ColorChoice::Color(n)) = cp::color_box(
                ui,
                (ui.scope_id(), "fx_color", f.key),
                egui::vec2(28.0, 16.0),
                Some(shown),
                Footer::Plain,
            ) {
                let n = cp::to_rgba(n);
                // The pop-up has no opacity for effect colours (Fireworks CS6); the shadow's
                // opacity is its own field below and stays as it was.
                arr = [n.r, n.g, n.b, if alpha { c.a } else { 255 }];
                changed = true;
            }
            if alpha {
                // Fireworks' drop shadow "Opacity" (the colour's alpha, in %).
                ui.label("Opacity:");
                let mut pct = (c.a as f64 * 100.0 / 255.0).round();
                if ui
                    .add(
                        egui::DragValue::new(&mut pct)
                            .range(0.0..=100.0)
                            .max_decimals(0)
                            .suffix("%"),
                    )
                    .changed()
                {
                    arr[3] = (pct * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8;
                    changed = true;
                }
            }
            if changed {
                out = Some(if matches!(f.kind, Kind::ColorAlpha) {
                    format!("#{:02x}{:02x}{:02x}{:02x}", arr[0], arr[1], arr[2], arr[3])
                } else {
                    format!("#{:02x}{:02x}{:02x}", arr[0], arr[1], arr[2])
                });
            }
        }
        Kind::Choice(options) => {
            let current = options
                .iter()
                .find(|(v, _)| *v == raw.trim())
                .map_or(raw.as_str(), |(_, l)| l)
                .to_owned();
            ComboBox::from_id_salt(("fx_choice", f.key))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (v, l) in options {
                        if ui.selectable_label(raw.trim() == *v, *l).clicked() {
                            out = Some((*v).to_owned());
                        }
                    }
                });
        }
        Kind::PluginPixels { min, max } => {
            let mut v = plugin.pixels(f.key, min as f32) as f64;
            if ui
                .add(fx_slider(&mut v, min, max, true).max_decimals(2).suffix(" px"))
                .changed()
            {
                out = Some(format!("{} 0", hex(&format!("{v:.2} pixels"))));
            }
        }
        Kind::PluginColor => {
            use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
            let c = plugin.color(f.key, Rgba::BLACK);
            if let Some(ColorChoice::Color(n)) = cp::color_box(
                ui,
                (ui.scope_id(), "plugin_color", f.key),
                egui::vec2(28.0, 16.0),
                Some(Rgba { a: 255, ..c }),
                Footer::Plain,
            ) {
                let n = cp::to_rgba(n);
                let rgb = [n.r, n.g, n.b];
                let pct = |v: u8| v as f64 / 2.55;
                let t = format!(
                    "RGB: {:.1}% {:.1}% {:.1}% ({} {} {})",
                    pct(rgb[0]),
                    pct(rgb[1]),
                    pct(rgb[2]),
                    rgb[0],
                    rgb[1],
                    rgb[2]
                );
                out = Some(format!("{} 0", hex(&t)));
            }
        }
        Kind::PluginEnum(ty, options) => {
            let cur = plugin.enumeration(f.key).unwrap_or_default();
            let label = options
                .iter()
                .find(|(v, _)| *v == cur)
                .map_or(cur.as_str(), |(_, l)| l)
                .to_owned();
            ComboBox::from_id_salt(("fx_enum", f.key))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (v, l) in options {
                        if ui.selectable_label(cur == *v, *l).clicked() {
                            out = Some(format!("{} {}", hex(ty), hex(v)));
                        }
                    }
                });
        }
    }
    ui.end_row();
    match out {
        Some(v) => {
            e.set(&key, v);
            true
        }
        None => false,
    }
}

/// The effects list in the Property inspector (right column).
pub fn panel(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else { return };
    let doc_id = d.id;
    let Some(list) = d.selected_effects() else { return };
    let state = &mut app.effects_ui;
    if state.selected.is_some_and(|i| i >= list.len()) {
        state.selected = None;
    }
    let mut new_list: Option<(Vec<EffectEntry>, &'static str)> = None;
    let mut open_editor: Option<usize> = None;
    // An effect with settings is added with its popup open: Cancel there removes it again.
    let mut adding = false;
    ui.horizontal(|ui| {
        ui.label("Effects:");
        ui.menu_button("+", |ui| {
            let mut categories: Vec<&str> = Vec::new();
            for d in DEFS {
                if !categories.contains(&d.category) {
                    categories.push(d.category);
                }
            }
            for cat in categories {
                ui.menu_button(cat, |ui| {
                    for d in DEFS.iter().filter(|d| d.category == cat) {
                        if ui.button(d.name).clicked() {
                            let mut l = list.clone();
                            l.push(create(d));
                            if has_editor(d) {
                                open_editor = Some(l.len() - 1);
                                adding = true;
                            }
                            new_list = Some((l, "Add Effect"));
                            ui.close();
                        }
                    }
                });
            }
        })
        .response
        .on_hover_text("Add an effect");
        let sel = state.selected;
        if ui
            .add_enabled(sel.is_some(), egui::Button::new("-").small())
            .on_hover_text("Delete the selected effect")
            .clicked()
            && let Some(i) = sel
        {
            let mut l = list.clone();
            l.remove(i);
            new_list = Some((l, "Delete Effect"));
            state.selected = None;
        }
        let up = sel.is_some_and(|i| i > 0);
        let down = sel.is_some_and(|i| i + 1 < list.len());
        if crate::icons::glyph_button(ui, crate::icons::Glyph::TriangleUp, 16.0, up)
            .on_hover_text("Move up (applied earlier)")
            .clicked()
            && up
            && let Some(i) = sel
        {
            let mut l = list.clone();
            l.swap(i, i - 1);
            state.selected = Some(i - 1);
            new_list = Some((l, "Reorder Effects"));
        }
        if crate::icons::glyph_button(ui, crate::icons::Glyph::TriangleDown, 16.0, down)
            .on_hover_text("Move down (applied later)")
            .clicked()
            && down
            && let Some(i) = sel
        {
            let mut l = list.clone();
            l.swap(i, i + 1);
            state.selected = Some(i + 1);
            new_list = Some((l, "Reorder Effects"));
        }
    });
    // The list: checkbox (on/off), name, (i) for settings.
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            ui.set_width(190.0);
            ui.set_min_height(40.0);
            egui::ScrollArea::vertical().max_height(70.0).show(ui, |ui| {
                for (i, e) in list.iter().enumerate() {
                    let known = e.effect.is_some();
                    let editable = def_for(e).is_some_and(has_editor);
                    let selected = state.selected == Some(i);
                    let row = ui.horizontal(|ui| {
                        let mut on = e.visible();
                        if ui
                            .checkbox(&mut on, "")
                            .on_hover_text("Show/hide this effect")
                            .changed()
                        {
                            let mut l = list.clone();
                            if let Some(x) = l.get_mut(i) {
                                x.set_visible(on);
                            }
                            new_list = Some((l, "Effect On/Off"));
                        }
                        let mut text = egui::RichText::new(e.name());
                        if !known {
                            text = text.color(Color32::GRAY);
                        }
                        if selected {
                            text = text.background_color(theme::SELECT_BLUE).color(Color32::WHITE);
                        }
                        let name = ui.add(egui::Label::new(text).sense(Sense::click()));
                        let name = if known {
                            name
                        } else {
                            name.on_hover_text("Kept in the file; FireCraft doesn't render this effect yet")
                        };
                        if name.clicked() {
                            state.selected = Some(i);
                        }
                        if name.double_clicked() && editable {
                            open_editor = Some(i);
                        }
                        if editable
                            && ui
                                .add(egui::Button::new("i").small())
                                .on_hover_text("Edit settings")
                                .clicked()
                        {
                            state.selected = Some(i);
                            open_editor = Some(i);
                        }
                    });
                    let _ = row;
                }
                if list.is_empty() {
                    ui.weak("No effects. Click + to add one.");
                }
            });
        });
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    let selection = d.selection.clone();
    // Levels and Curves show the object's histogram as it reaches the effect.
    if let Some(i) = open_editor {
        let tables = new_list
            .as_ref()
            .map_or(list.get(i), |(l, _)| l.get(i))
            .and_then(def_for)
            .is_some_and(|d| d.name == LEVELS || d.name == CURVES);
        app.effects_ui.tables = crate::tables_ui::TableEditor::new(
            tables.then(|| d.effect_input(i)).flatten().as_ref(),
        );
    }
    match (new_list, open_editor) {
        (Some((l, _)), Some(i)) if adding => {
            d.begin_live_edit("Add Effect");
            d.set_effects_live(l);
            app.effects_ui.selected = Some(i);
            app.effects_ui.editing = Some(Editing {
                doc: doc_id,
                index: i,
                selection,
                original: list,
                added: true,
            });
        }
        (new_list, open_editor) => {
            if let Some((l, label)) = new_list {
                d.set_effects(l, label);
            }
            if let Some(i) = open_editor {
                let original = d.selected_effects().unwrap_or_default();
                d.begin_live_edit("Edit Effect");
                app.effects_ui.selected = Some(i);
                app.effects_ui.editing = Some(Editing {
                    doc: doc_id,
                    index: i,
                    selection,
                    original,
                    added: false,
                });
            }
        }
    }
}

/// The Filters menu's categories and filters (the effects that are filters, as in Fireworks:
/// Adjust Color, Blur, Other, Sharpen and the plug-ins).
pub fn filters_menu(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let can = app.session.active_doc().is_some_and(|d| d.can_filter());
    let repeat = app.effects_ui.last_filter.as_ref().map(EffectEntry::name);
    let label = repeat.as_deref().map_or("Repeat Filter".to_owned(), |n| {
        format!("Repeat {}", n.trim_end_matches("..."))
    });
    if crate::menus::item(ui, &label, "Ctrl+Alt+Shift+X", can && repeat.is_some()) {
        repeat_filter(app);
    }
    ui.separator();
    let mut categories: Vec<&str> = Vec::new();
    for d in DEFS.iter().filter(|d| d.filter) {
        if !categories.contains(&d.category) {
            categories.push(d.category);
        }
    }
    // Fireworks' own filters, then the plug-ins.
    let mut plugins_started = false;
    for cat in categories {
        let plugin = DEFS.iter().any(|d| d.category == cat && d.plugin_category.is_some());
        if plugin && !plugins_started {
            plugins_started = true;
            ui.separator();
        }
        ui.add_enabled_ui(can, |ui| {
            ui.menu_button(cat, |ui| {
                for d in DEFS.iter().filter(|d| d.filter && d.category == cat) {
                    if crate::menus::item(ui, d.name, "", true) {
                        start_filter(app, d);
                    }
                }
            });
        });
    }
}

/// A Filters menu choice: applied at once when it has no settings, otherwise its popup opens.
fn start_filter(app: &mut FireCraftApp, d: &'static EffectDef) {
    let Some(doc) = app.session.active_doc_mut() else { return };
    // The last settings of the same filter, as Fireworks remembers them.
    let mut entry = match &app.effects_ui.last_filter {
        Some(e) if e.name() == d.name && e.moa_id() == d.moa => e.clone(),
        _ => create(d),
    };
    // The menu's Sharpen and Sharpen More are strengths 4 and 8 (FirePixel's reading of
    // Fireworks; not yet checked against Fireworks MX). As Live Effects both are 1 (measured).
    match d.name {
        "Sharpen" => entry.set("sharpen_amount", "4"),
        "Sharpen More" => entry.set("sharpen_amount", "8"),
        _ => {}
    }
    let converts = doc.filter_converts();
    if !has_editor(d) && !converts {
        if let Some(fx) = entry.effect() {
            doc.apply_filter(fx, d.name.trim_end_matches("..."));
        }
        app.effects_ui.last_filter = Some(entry);
        return;
    }
    if d.name == LEVELS || d.name == CURVES {
        app.effects_ui.tables = crate::tables_ui::TableEditor::new(doc.filter_input().as_ref());
    }
    app.effects_ui.filter = Some(FilterDialog {
        doc: doc.id,
        entry,
        confirmed: !converts,
        started: false,
    });
}

/// Filters > Repeat Filter: the last filter again, with the same settings.
fn repeat_filter(app: &mut FireCraftApp) {
    let Some(e) = app.effects_ui.last_filter.clone() else { return };
    if let (Some(d), Some(fx)) = (app.session.active_doc_mut(), e.effect()) {
        d.apply_filter(fx, e.name().trim_end_matches("..."));
    }
}

/// The Filters menu's keyboard shortcut (Repeat Filter).
pub fn filter_shortcuts(app: &mut FireCraftApp, ctx: &egui::Context) {
    let m = egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT;
    if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(m, egui::Key::X)))
        && app.session.active_doc().is_some_and(|d| d.can_filter())
    {
        repeat_filter(app);
    }
}

/// The Filters menu's popup: Fireworks' question before converting objects to bitmaps, then the
/// filter's settings, previewed on the canvas. OK applies (one undo step), Cancel restores.
pub fn filter_dialog(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut fd) = app.effects_ui.filter.take() else { return };
    let Some(d) = app.session.docs.iter_mut().find(|d| d.id == fd.doc) else {
        return;
    };
    let def = def_for(&fd.entry);
    let label = fd.entry.name().trim_end_matches("...").to_owned();
    let (mut ok, mut cancel) =
        ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    if !fd.confirmed {
        egui::Window::new("Fireworks")
            .id(egui::Id::unique("filter_convert"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Applying filters will convert the selection to a bitmap. Continue?");
                ui.horizontal(|ui| {
                    ok |= ui.button("OK").clicked();
                    cancel |= ui.button("Cancel").clicked();
                });
            });
        if cancel {
            return;
        }
        if ok {
            fd.confirmed = true;
            if !def.is_some_and(has_editor) {
                if let Some(fx) = fd.entry.effect() {
                    d.apply_filter(fx, &label);
                }
                app.effects_ui.last_filter = Some(fd.entry);
                return;
            }
            // The popup opens next frame (Enter must not also press its OK).
            app.effects_ui.filter = Some(fd);
            return;
        }
        app.effects_ui.filter = Some(fd);
        return;
    }
    let Some(def) = def else { return };
    if !fd.started {
        fd.started = true;
        d.begin_live_edit(&label);
        if let Some(fx) = fd.entry.effect() {
            d.preview_filter(fx);
        }
    }
    let mut changed = false;
    let mut open = true;
    egui::Window::new(label.clone())
        .id(egui::Id::unique("filter_dialog"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - vec2(160.0, 200.0))
        .show(ctx, |ui| {
            ui.spacing_mut().slider_width = 240.0;
            match def.name {
                LEVELS => changed |= crate::tables_ui::levels(ui, &mut fd.entry, &mut app.effects_ui.tables),
                CURVES => changed |= crate::tables_ui::curves(ui, &mut fd.entry, &mut app.effects_ui.tables),
                _ => {
                    egui::Grid::new("filter_fields")
                        .num_columns(2)
                        .spacing(vec2(8.0, 4.0))
                        .show(ui, |ui| {
                            for f in def.fields {
                                changed |= edit_field(ui, def, f, &mut fd.entry);
                            }
                        });
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel |= ui.button("Cancel").clicked();
            });
        });
    cancel |= !open;
    if changed && let Some(fx) = fd.entry.effect() {
        d.preview_filter(fx);
    }
    if cancel {
        d.cancel_live_edit();
    } else if ok {
        d.commit_live_edit();
        app.effects_ui.last_filter = Some(fd.entry);
    } else {
        app.effects_ui.filter = Some(fd);
    }
}

/// The settings popup of the effect being edited. Changes show live; OK (or Enter) keeps them,
/// Cancel (or Escape, or the close button) undoes them.
pub fn editor(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(ed) = app.effects_ui.editing.clone() else {
        return;
    };
    let Some(d) = app.session.active_doc_mut() else {
        app.effects_ui.editing = None;
        return;
    };
    // Another document, or another selection: keep what was done so far.
    let entry = d.selected_effects().and_then(|l| l.get(ed.index).cloned());
    let def = entry.as_ref().and_then(def_for);
    let (Some(mut e), Some(def)) = (entry, def) else {
        app.effects_ui.editing = None;
        return;
    };
    if d.id != ed.doc || d.selection != ed.selection {
        app.effects_ui.editing = None;
        return;
    }
    let mut changed = false;
    let (mut ok, mut cancel) =
        ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    let mut open = true;
    egui::Window::new(e.name().trim_end_matches("...").to_owned())
        .id(egui::Id::unique("effect_editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().left_bottom() + vec2(320.0, -330.0))
        .show(ctx, |ui| {
            // Long sliders for fine control (240 px: a 0-100 setting moves ~0.4 per pixel).
            ui.spacing_mut().slider_width = 240.0;
            match def.name {
                LEVELS => changed |= crate::tables_ui::levels(ui, &mut e, &mut app.effects_ui.tables),
                CURVES => changed |= crate::tables_ui::curves(ui, &mut e, &mut app.effects_ui.tables),
                _ => {
                    egui::Grid::new("fx_fields")
                        .num_columns(2)
                        .spacing(vec2(8.0, 4.0))
                        .show(ui, |ui| {
                            for f in def.fields {
                                changed |= edit_field(ui, def, f, &mut e);
                            }
                        });
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel |= ui.button("Cancel").clicked();
            });
        });
    cancel |= !open;
    if changed && let Some(mut list) = d.selected_effects() {
        if let Some(slot) = list.get_mut(ed.index) {
            *slot = e;
        }
        d.set_effects_live(list);
    }
    if cancel {
        d.cancel_live_edit();
        app.effects_ui.editing = None;
        if ed.added {
            app.effects_ui.selected = None;
        }
    } else if ok {
        // Nothing changed: no undo step.
        if !ed.added && d.selected_effects().as_ref() == Some(&ed.original) {
            d.discard_live_edit();
        }
        d.commit_live_edit();
        app.effects_ui.editing = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_effects_render_and_round_trip() {
        for d in DEFS {
            let e = create(d);
            assert!(e.effect.is_some(), "{} is not rendered", d.name);
            assert_eq!(def_for(&e).map(|x| x.name), Some(d.name), "{}", d.name);
            assert_eq!(EffectEntry::read(&e.write()), e);
        }
    }

    #[test]
    fn plugin_encodings() {
        assert_eq!(hex("10.00 pixels"), "31302e303020706978656c73");
        let d = DEFS.iter().find(|d| d.name == "Bevel Boss...").unwrap();
        let f = d.fields.iter().find(|f| f.key == "Fl  ").unwrap();
        assert_eq!(key_for(d, f), "external_filter_key__0x466c2020");
    }
}
