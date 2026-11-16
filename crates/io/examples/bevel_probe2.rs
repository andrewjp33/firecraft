//! Probe document for Bevel Boss's highlight falloff and profile curves: bevel_probe2 <out.png>
//!
//! Every object is a 300 px disc (1 px soft edge) of (255, 0, 255), so the green channel holds the
//! highlight alone (and a Wine render that loses blue stays usable). Smoothness 0, light 135 / 45.
//! Row 0, cones of constant slope (profile 0 -> 1 straight, Width 1000, Height 28, about 22.5
//! degrees: every angle to the half vector from 0 to 45 shows around the cone), Highlight Size
//! 10, 25, 50, 75, 100. Row 1, Width 150, Height 50, Highlight Size 25, profiles (flag 1 = sharp):
//! four smooth points; five smooth points; three with a sharp middle; four mixed; two (straight).
//! Cells 340x340.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

const R: f32 = 150.0;

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn profile(points: &[(f32, f32, u8)]) -> String {
    let body: Vec<String> = points.iter().map(|(x, y, f)| format!("{x:.3} {y:.3} {f}")).collect();
    format!("{} 0", hex(&format!("Bevel: {}/{}", points.len(), body.join("/"))))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: bevel_probe2 <out.png>")?;
    let n = (2.0 * R) as u32;
    let mut disc = RgbaImage::new(n, n, Rgba::TRANSPARENT);
    for y in 0..n {
        for x in 0..n {
            let d = (x as f32 + 0.5 - R).hypot(y as f32 + 0.5 - R);
            let a = (R - d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                disc.set(x as i64, y as i64, Rgba::new(255, 0, 255, (a * 255.0).round() as u8));
            }
        }
    }
    let line = profile(&[(0.0, 1.0, 0), (1.0, 0.0, 0)]);
    // (label, width, height, highlight size, profile)
    let mut cells: Vec<(String, f32, f32, f32, String)> = [10.0, 25.0, 50.0, 75.0, 100.0]
        .iter()
        .map(|s| (format!("cone size {s}"), 1000.0, 28.0, *s, line.clone()))
        .collect();
    let profiles: [(&str, Vec<(f32, f32, u8)>); 5] = [
        ("4 smooth", vec![(0.0, 1.0, 0), (0.3, 0.9, 0), (0.7, 0.4, 0), (1.0, 0.0, 0)]),
        ("5 smooth", vec![(0.0, 0.0, 0), (0.25, 0.8, 0), (0.5, 0.3, 0), (0.75, 0.9, 0), (1.0, 0.1, 0)]),
        ("3 sharp mid", vec![(0.0, 1.0, 0), (0.5, 0.3, 1), (1.0, 0.0, 0)]),
        ("4 mixed", vec![(0.0, 0.2, 0), (0.4, 1.0, 1), (0.6, 0.5, 0), (1.0, 0.0, 0)]),
        ("2 line", vec![(0.0, 1.0, 0), (1.0, 0.0, 0)]),
    ];
    for (label, pts) in profiles {
        cells.push((label.into(), 150.0, 50.0, 25.0, profile(&pts)));
    }
    let (cell, cols) = (340u32, 5u32);
    let mut doc = Document::new(cell * cols, cell * 2, 72.0, None);
    let t = |s: &str| EffectParam::Text(s.into());
    for (i, (label, width, height, size, prof)) in cells.into_iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        let (x, y) = ((cx * cell + 20) as i64, (cy * cell + 20) as i64);
        let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: disc.clone() }));
        el.name = Some(label);
        el.effects.push(EffectEntry::new(vec![
            ("mkbFile_WriteOnly_TemporaryEffectUiName".into(), t("Bevel Boss...")),
            ("external_filter_have_scripting_data".into(), t("true")),
            ("name".into(), t("Bevel Boss...")),
            ("category".into(), t(" Eye Candy 4000 LE")),
            ("EffectIsVisible".into(), t("true")),
            ("EffectMoaID".into(), t("{d810e821-f86b-11d0-656341346564694e}")),
            ("MB_filter_preview_tile_size".into(), t("-1 -1")),
            ("external_filter_key__Wdth".into(), t(&format!("{} 0", hex(&format!("{width:.2} pixels"))))),
            ("external_filter_key__Cntr".into(), t(&format!("{height}"))),
            ("external_filter_key__Smth".into(), t("0")),
            ("external_filter_key__bvlS".into(), t("42657469 62655430")),
            ("external_filter_key__0x466c2020".into(), t("true")),
            ("external_filter_key__Drct".into(), t("135")),
            ("external_filter_key__INcl".into(), t("45")),
            ("external_filter_key__Brgh".into(), t("100")),
            ("external_filter_key__Shrp".into(), t(&format!("{size}"))),
            (
                "external_filter_key__0x436c7220".into(),
                t("5247423a203130302e3025203130302e3025203130302e30252028323535203235352032353529 0"),
            ),
            ("external_filter_key__sdwC".into(), t("5247423a20302e302520302e302520302e30252028302030203029 0")),
            ("external_filter_key__BEvl".into(), t(&prof)),
            ("external_filter_key__Drkn".into(), t("0")),
        ]));
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v.push(el);
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("10 cells -> {out}");
    Ok(())
}
