//! Probe document for Eye Candy 4000 LE Bevel Boss, for rendering in Fireworks MX on Windows
//! (under Wine the plug-in loses blue): bevel_probe <out.png>
//!
//! One row per content, one column per setting; cells 200x200, content centred at (100, 100).
//! Contents (all 120x120 bitmaps):
//! - hard: a disc of radius 50 with a 1 px soft edge, colours varying with angle;
//! - halo: the same disc with alpha falling to 0 over its outer 16 px;
//! - ramp: a square whose alpha rises 40 → 220 left to right, with a soft ellipse of lower
//!   alpha (like spectrum_wave.png after Convert to Alpha);
//! - ring white / ring black: a disc with a transparent round hole of radius 20 whose
//!   transparent pixels store white or black (does their colour reach the bevel?);
//! - square hole: a solid square with a transparent square hole in it.
//!
//! Settings: base (Width 15, Height 50, Smoothness 50, inside, light 135/45), Smoothness 0,
//! Smoothness 100, Width 5, Width 30, outside.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

const N: u32 = 120;

fn hue(ang: f32) -> [f32; 3] {
    let c = |o: f32| 128.0 + 100.0 * (ang + o).cos();
    [c(0.0), c(2.094), c(4.189)]
}

fn content(kind: &str) -> RgbaImage {
    let mut img = RgbaImage::new(N, N, Rgba::TRANSPARENT);
    let c = N as f32 / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = dx.hypot(dy);
            let [r, g, b] = hue(dy.atan2(dx));
            let col = |a: f32| Rgba::new(r as u8, g as u8, b as u8, (a.clamp(0.0, 1.0) * 255.0).round() as u8);
            let p = match kind {
                "hard" => col(50.0 - d + 0.5),
                "halo" => col(if d <= 34.0 { 1.0 } else { 1.0 - (d - 34.0) / 16.0 }),
                "ramp" => {
                    let base = 40.0 + 180.0 * x as f32 / (N - 1) as f32;
                    // A soft ellipse 70 alpha levels lower.
                    let e = ((dx + 10.0) / 28.0).hypot(dy / 40.0);
                    let dip = 70.0 * (1.0 - ((e - 0.8) / 0.4).clamp(0.0, 1.0));
                    Rgba::new(40, 90, 160, (base - dip).round() as u8)
                }
                "ring white" | "ring black" => {
                    let a = (50.0 - d + 0.5).clamp(0.0, 1.0) * (d - 20.0 + 0.5).clamp(0.0, 1.0);
                    if a <= 0.0 {
                        if kind == "ring white" { Rgba::new(255, 255, 255, 0) } else { Rgba::new(0, 0, 0, 0) }
                    } else {
                        col(a)
                    }
                }
                _ => {
                    // Square hole: solid square 10..110 with a transparent hole 40..80.
                    let inside = |v: u32| (10..110).contains(&v);
                    let hole = |v: u32| (40..80).contains(&v);
                    if inside(x) && inside(y) && !(hole(x) && hole(y)) {
                        Rgba::new(200, 120, 60, 255)
                    } else {
                        Rgba::TRANSPARENT
                    }
                }
            };
            img.set(x as i64, y as i64, p);
        }
    }
    img
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: bevel_probe <out.png>")?;
    let contents = ["hard", "halo", "ramp", "ring white", "ring black", "square hole"];
    // (label, width, smoothness, outside)
    let settings: [(&str, f32, f32, bool); 6] = [
        ("base", 15.0, 50.0, false),
        ("smooth 0", 15.0, 0.0, false),
        ("smooth 100", 15.0, 100.0, false),
        ("width 5", 5.0, 50.0, false),
        ("width 30", 30.0, 50.0, false),
        ("outside", 15.0, 50.0, true),
    ];
    let cell = 200u32;
    let mut doc = Document::new(cell * settings.len() as u32, cell * contents.len() as u32, 72.0, None);
    let t = |s: &str| EffectParam::Text(s.into());
    for (row, kind) in contents.iter().enumerate() {
        let img = content(kind);
        for (col, (label, width, smooth, outside)) in settings.iter().enumerate() {
            let (x, y) = ((col as u32 * cell + (cell - N) / 2) as i64, (row as u32 * cell + (cell - N) / 2) as i64);
            let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img.clone() }));
            el.name = Some(format!("{kind} {label}"));
            el.effects.push(EffectEntry::new(vec![
                ("mkbFile_WriteOnly_TemporaryEffectUiName".into(), t("Bevel Boss...")),
                ("external_filter_have_scripting_data".into(), t("true")),
                ("name".into(), t("Bevel Boss...")),
                ("category".into(), t(" Eye Candy 4000 LE")),
                ("EffectIsVisible".into(), t("true")),
                ("EffectMoaID".into(), t("{d810e821-f86b-11d0-656341346564694e}")),
                ("MB_filter_preview_tile_size".into(), t("-1 -1")),
                ("external_filter_key__Wdth".into(), t(&format!("{} 0", hex(&format!("{width:.2} pixels"))))),
                ("external_filter_key__Cntr".into(), t("50")),
                ("external_filter_key__Smth".into(), t(&format!("{smooth}"))),
                ("external_filter_key__bvlS".into(), t(if *outside { "42657469 62655431" } else { "42657469 62655430" })),
                ("external_filter_key__0x466c2020".into(), t("true")),
                ("external_filter_key__Drct".into(), t("135")),
                ("external_filter_key__INcl".into(), t("45")),
                ("external_filter_key__Brgh".into(), t("100")),
                ("external_filter_key__Shrp".into(), t("25")),
                (
                    "external_filter_key__0x436c7220".into(),
                    t("5247423a203130302e3025203130302e3025203130302e30252028323535203235352032353529 0"),
                ),
                ("external_filter_key__sdwC".into(), t("5247423a20302e302520302e302520302e30252028302030203029 0")),
                (
                    "external_filter_key__BEvl".into(),
                    t("426576656c3a20332f302e30303020302e30303020302f302e36373120302e34353820302f312e30303020312e3030302030 0"),
                ),
                ("external_filter_key__Drkn".into(), t("0")),
            ]));
            if let Some(v) = doc.layers[0].elements_mut(0) {
                v.push(el);
            }
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!(
        "{} rows ({}) x {} columns ({}) of {cell}px -> {out}",
        contents.len(),
        contents.join(", "),
        settings.len(),
        settings.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")
    );
    Ok(())
}
