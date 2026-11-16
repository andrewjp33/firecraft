//! Probe document for Eye Candy 4000 LE Motion Trail: one cell per setting: trail_probe <out.png>
//!
//! Content: a 60 px disc with a 1 px soft edge on transparency, colours with red = blue (Fireworks
//! MX under Wine runs the plug-in with red and blue swapped on the first render, so those stay
//! comparable), green varying with angle and a darker ring. Cells are 300x200, disc centred at
//! (90, 100). Compare Fireworks MX's re-save with cell_compare <file> 300 200.
//!
//! Cells 10.. use a second disc for the trail's colour: full RGB hues changing around the edge
//! and a 12 px soft halo (alpha falling to 0 outward), like SOLAR.png's planets. Render those on
//! Windows: under Wine the plug-in loses blue.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

const R: f32 = 30.0;

fn disc() -> RgbaImage {
    let n = (2.0 * R) as u32;
    let mut img = RgbaImage::new(n, n, Rgba::TRANSPARENT);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - R, y as f32 + 0.5 - R);
            let d = dx.hypot(dy);
            let cover = (R - d + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let ang = dy.atan2(dx);
            let g = (127.5 + 127.5 * ang.cos()) as u8;
            let rb = if (d as u32 / 6) % 2 == 0 { 220 } else { 60 };
            img.set(
                x as i64,
                y as i64,
                Rgba::new(rb, g, rb, (cover * 255.0).round() as u8),
            );
        }
    }
    img
}

const HALO: f32 = 12.0;

/// A disc of radius R with a soft halo out to R + HALO; hue by angle (six sectors with sharp
/// borders), lighter towards the centre.
fn halo_disc() -> RgbaImage {
    let n = (2.0 * (R + HALO)) as u32;
    let c = R + HALO;
    let mut img = RgbaImage::new(n, n, Rgba::TRANSPARENT);
    let hues = [
        [230, 40, 40],
        [240, 200, 30],
        [40, 200, 60],
        [30, 200, 220],
        [40, 60, 230],
        [210, 50, 210],
    ];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = dx.hypot(dy);
            let a = if d <= R { 1.0 } else { (1.0 - (d - R) / HALO).clamp(0.0, 1.0) };
            if a <= 0.0 {
                continue;
            }
            let ang = dy.atan2(dx).to_degrees() + 180.0;
            let h = hues[((ang / 60.0) as usize).min(5)];
            let t = (d / R).min(1.0);
            let ch = |v: u8| (255.0 + (v as f32 - 255.0) * (0.4 + 0.6 * t)).round() as u8;
            img.set(x as i64, y as i64, Rgba::new(ch(h[0] as u8), ch(h[1] as u8), ch(h[2] as u8), (a * 255.0).round() as u8));
        }
    }
    img
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: trail_probe <out.png>")?;
    // (label, direction, length, taper, opacity, smear, outside only)
    let cells: [(&str, f32, f32, f32, f32, bool, bool); 16] = [
        ("base", 0.0, 40.0, 40.0, 95.0, true, false),
        ("taper 0", 0.0, 40.0, 0.0, 95.0, true, false),
        ("taper 100", 0.0, 40.0, 100.0, 95.0, true, false),
        ("len 108", 0.0, 108.0, 40.0, 95.0, true, false),
        ("opacity 50", 0.0, 40.0, 40.0, 50.0, true, false),
        ("no smear", 0.0, 40.0, 40.0, 95.0, false, false),
        ("dir 33", 33.0, 40.0, 40.0, 95.0, true, false),
        ("dir 90", 90.0, 40.0, 40.0, 95.0, true, false),
        ("outside only", 0.0, 40.0, 40.0, 95.0, true, true),
        ("len 108 taper 0", 0.0, 108.0, 0.0, 100.0, true, false),
        ("halo base", 0.0, 40.0, 40.0, 95.0, true, false),
        ("halo no smear", 0.0, 40.0, 40.0, 95.0, false, false),
        ("halo len 108", 0.0, 108.0, 40.0, 95.0, true, false),
        ("halo taper 0", 0.0, 40.0, 0.0, 95.0, true, false),
        ("halo dir 331", 331.0, 108.0, 40.0, 95.0, true, false),
        ("halo dir 90", 90.0, 40.0, 40.0, 95.0, true, false),
    ];
    let (cw, ch, cols) = (300u32, 200u32, 2u32);
    let rows = (cells.len() as u32).div_ceil(cols);
    let mut doc = Document::new(cw * cols, ch * rows, 72.0, None);
    let (plain, halo) = (disc(), halo_disc());
    for (i, (label, dir, len, taper, op, smear, outside)) in cells.into_iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        let img = if i < 10 { &plain } else { &halo };
        let r = img.width() as i64 / 2;
        let (x, y) = ((cx * cw + 90) as i64 - r, (cy * ch + 100) as i64 - r);
        let mut el = Element::new(ElementKind::Bitmap(Bitmap {
            x,
            y,
            image: img.clone(),
        }));
        el.name = Some(label.into());
        let t = |s: &str| EffectParam::Text(s.into());
        let b = |v: bool| t(if v { "true" } else { "false" });
        el.effects.push(EffectEntry::new(vec![
            (
                "mkbFile_WriteOnly_TemporaryEffectUiName".into(),
                t("Motion Trail..."),
            ),
            ("external_filter_have_scripting_data".into(), t("true")),
            ("name".into(), t("Motion Trail...")),
            ("category".into(), t(" Eye Candy 4000 LE")),
            ("EffectIsVisible".into(), t("true")),
            ("EffectMoaID".into(), t("{d810e821-f86b-11d0-6563413465646d54}")),
            ("MB_filter_preview_tile_size".into(), t("-1 -1")),
            ("external_filter_key__Drct".into(), t(&format!("{dir}"))),
            (
                "external_filter_key__Lngt".into(),
                t(&format!("{} 0", hex(&format!("{len:.2} pixels")))),
            ),
            ("external_filter_key__TApe".into(), t(&format!("{taper}"))),
            ("external_filter_key__0x45646720".into(), b(smear)),
            ("external_filter_key__0x466c2020".into(), b(outside)),
            ("external_filter_key__Opct".into(), t(&format!("{op}"))),
        ]));
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v.push(el);
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{} cells of {cw}x{ch}, {cols} per row -> {out}", cells.len());
    Ok(())
}
