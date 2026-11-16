//! Probe document for Bevel Boss Smoothness on a large dome (eye2.png's bevel, scaled down):
//! smooth_probe <out.png>
//!
//! One 400 px disc (1 px soft edge) per cell, colour (200, 120, 200) (red = blue, so a Wine render
//! that loses blue stays comparable), Bevel Boss as eye2.png's (profile 0.261 / 0.225 / 0, Height
//! 76, light 135 / 45) with Width 171 (eye2's 428 / 500 radius ratio) and Smoothness 0, 5, 10, 20,
//! 35, 50, 75, 100. Cells 440x440, four per row.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

const R: f32 = 200.0;

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: smooth_probe <out.png>")?;
    let n = (2.0 * R) as u32;
    let mut disc = RgbaImage::new(n, n, Rgba::TRANSPARENT);
    for y in 0..n {
        for x in 0..n {
            let d = (x as f32 + 0.5 - R).hypot(y as f32 + 0.5 - R);
            let a = (R - d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                disc.set(x as i64, y as i64, Rgba::new(200, 120, 200, (a * 255.0).round() as u8));
            }
        }
    }
    // eye2.png's profile: Bevel: 3/0.000 0.261 0/0.401 0.225 0/1.000 0.000 0
    let profile = hex("Bevel: 3/0.000 0.261 0/0.401 0.225 0/1.000 0.000 0");
    let smooth = [0, 5, 10, 20, 35, 50, 75, 100];
    let (cell, cols) = (440u32, 4u32);
    let rows = (smooth.len() as u32).div_ceil(cols);
    let mut doc = Document::new(cell * cols, cell * rows, 72.0, None);
    let t = |s: &str| EffectParam::Text(s.into());
    for (i, sm) in smooth.iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        let (x, y) = ((cx * cell + 20) as i64, (cy * cell + 20) as i64);
        let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: disc.clone() }));
        el.name = Some(format!("smooth {sm}"));
        el.effects.push(EffectEntry::new(vec![
            ("mkbFile_WriteOnly_TemporaryEffectUiName".into(), t("Bevel Boss...")),
            ("external_filter_have_scripting_data".into(), t("true")),
            ("name".into(), t("Bevel Boss...")),
            ("category".into(), t(" Eye Candy 4000 LE")),
            ("EffectIsVisible".into(), t("true")),
            ("EffectMoaID".into(), t("{d810e821-f86b-11d0-656341346564694e}")),
            ("MB_filter_preview_tile_size".into(), t("-1 -1")),
            ("external_filter_key__Wdth".into(), t(&format!("{} 0", hex("171.00 pixels")))),
            ("external_filter_key__Cntr".into(), t("76")),
            ("external_filter_key__Smth".into(), t(&sm.to_string())),
            ("external_filter_key__bvlS".into(), t("42657469 62655430")),
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
            ("external_filter_key__BEvl".into(), t(&format!("{profile} 0"))),
            ("external_filter_key__Drkn".into(), t("0")),
        ]));
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v.push(el);
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{} cells -> {out}", smooth.len());
    Ok(())
}
