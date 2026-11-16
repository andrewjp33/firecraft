//! Probe document for Bevel Boss on rectangles (straight edges give clean cross-sections):
//! rect_bevel_probe <out.png>
//!
//! Hard-edged 240x160 bitmaps of (255, 0, 255) (green = highlight only), Smoothness 0, light from
//! the left (direction 180, inclination 45), Highlight Size 25. Row 0: a straight profile (1 inside
//! to 0 at the edge), Width 40, Height Scale 10, 25, 50, 75, 100 (the height mapping). Row 1:
//! Width 80, Height 50, profiles (flag 1 = sharp): three smooth (eye's), four smooth, five smooth,
//! three with a sharp middle, four mixed. Cells 300x220.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn profile(points: &[(f32, f32, u8)]) -> String {
    let body: Vec<String> = points.iter().map(|(x, y, f)| format!("{x:.3} {y:.3} {f}")).collect();
    format!("{} 0", hex(&format!("Bevel: {}/{}", points.len(), body.join("/"))))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: rect_bevel_probe <out.png>")?;
    let rect = RgbaImage::new(240, 160, Rgba::new(255, 0, 255, 255));
    let line = profile(&[(0.0, 1.0, 0), (1.0, 0.0, 0)]);
    let mut cells: Vec<(String, f32, f32, String)> = [10.0, 25.0, 50.0, 75.0, 100.0]
        .iter()
        .map(|h| (format!("height {h}"), 40.0, *h, line.clone()))
        .collect();
    let profiles: [(&str, Vec<(f32, f32, u8)>); 5] = [
        ("3 smooth (eye)", vec![(0.0, 0.261, 0), (0.401, 0.225, 0), (1.0, 0.0, 0)]),
        ("4 smooth", vec![(0.0, 1.0, 0), (0.3, 0.9, 0), (0.7, 0.4, 0), (1.0, 0.0, 0)]),
        ("5 smooth", vec![(0.0, 0.0, 0), (0.25, 0.8, 0), (0.5, 0.3, 0), (0.75, 0.9, 0), (1.0, 0.1, 0)]),
        ("3 sharp mid", vec![(0.0, 1.0, 0), (0.5, 0.3, 1), (1.0, 0.0, 0)]),
        ("4 mixed", vec![(0.0, 0.2, 0), (0.4, 1.0, 1), (0.6, 0.5, 0), (1.0, 0.0, 0)]),
    ];
    for (label, pts) in profiles {
        cells.push((label.into(), 80.0, 50.0, profile(&pts)));
    }
    let (cw, ch, cols) = (300u32, 220u32, 5u32);
    let mut doc = Document::new(cw * cols, ch * 2, 72.0, None);
    let t = |s: &str| EffectParam::Text(s.into());
    for (i, (label, width, height, prof)) in cells.into_iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        let (x, y) = ((cx * cw + 30) as i64, (cy * ch + 30) as i64);
        let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: rect.clone() }));
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
            ("external_filter_key__Drct".into(), t("180")),
            ("external_filter_key__INcl".into(), t("45")),
            ("external_filter_key__Brgh".into(), t("100")),
            ("external_filter_key__Shrp".into(), t("25")),
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
