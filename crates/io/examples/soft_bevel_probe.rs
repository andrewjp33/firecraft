//! Probe document for Bevel Boss on soft alpha (as in spectrum_wave.png after Convert to Alpha),
//! for rendering in Fireworks MX on Windows: soft_bevel_probe <out.png>
//!
//! Cells 200x200, one 160x160 bitmap each, centred. Rows (content): radial alpha ramps whose
//! alpha-128 contour is a circle of radius 50, alpha = 128 + slope x (50 - r):
//! - gray, slope 2, 6 and 20 alpha levels per pixel (no noise);
//! - gray, slope 2 with uniform noise of +-5 and +-20 levels (deterministic hash);
//! - gray disc of radius 70 at alpha 180 with +-20 noise (relief on a flat soft area?);
//! - black, slope 2 (only the highlight shows, as in spectrum_wave.png).
//! Columns (Bevel Boss): spectrum_wave's settings (Width 5.76, Height 50, Smoothness 75, light
//! 222.4 / 51.5, its profile); the same at Smoothness 0; the same at Width 15; the same with the
//! light at 135 / 45.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

const N: u32 = 160;

fn noise(x: u32, y: u32) -> f32 {
    let h = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)).wrapping_mul(2_654_435_761);
    ((h >> 8) % 1000) as f32 / 999.0 * 2.0 - 1.0
}

fn content(row: usize) -> RgbaImage {
    let mut img = RgbaImage::new(N, N, Rgba::TRANSPARENT);
    let c = N as f32 / 2.0;
    for y in 0..N {
        for x in 0..N {
            let r = (x as f32 + 0.5 - c).hypot(y as f32 + 0.5 - c);
            let ramp = |slope: f32| 128.0 + slope * (50.0 - r);
            let (a, grey) = match row {
                0 => (ramp(2.0), 160),
                1 => (ramp(6.0), 160),
                2 => (ramp(20.0), 160),
                3 => (ramp(2.0) + 5.0 * noise(x, y), 160),
                4 => (ramp(2.0) + 20.0 * noise(x, y), 160),
                5 => (if r <= 70.0 { 180.0 + 20.0 * noise(x, y) } else { 0.0 }, 160),
                _ => (ramp(2.0), 0),
            };
            let a = a.round().clamp(0.0, 255.0) as u8;
            if a > 0 {
                img.set(x as i64, y as i64, Rgba::new(grey, grey, grey, a));
            }
        }
    }
    img
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: soft_bevel_probe <out.png>")?;
    // spectrum_wave.png's profile: Bevel: 3/0.000 1.000 0/0.500 0.660 0/1.000 0.000 0
    let profile = "426576656c3a20332f302e30303020312e30303020302f302e35303020302e36363020302f312e30303020302e3030302030 0";
    // (label, width, smoothness, direction, inclination)
    let cols: [(&str, f32, f32, &str, &str); 4] = [
        ("spectrum", 5.76, 75.0, "222.39743764026633", "51.472040830415693"),
        ("smooth 0", 5.76, 0.0, "222.39743764026633", "51.472040830415693"),
        ("width 15", 15.0, 75.0, "222.39743764026633", "51.472040830415693"),
        ("light 135/45", 5.76, 75.0, "135", "45"),
    ];
    let rows = 7usize;
    let cell = 200u32;
    let mut doc = Document::new(cell * cols.len() as u32, cell * rows as u32, 72.0, None);
    let t = |s: &str| EffectParam::Text(s.into());
    for row in 0..rows {
        let img = content(row);
        for (col, (label, width, smooth, dir, incl)) in cols.iter().enumerate() {
            let (x, y) = ((col as u32 * cell + (cell - N) / 2) as i64, (row as u32 * cell + (cell - N) / 2) as i64);
            let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img.clone() }));
            el.name = Some(format!("row {row} {label}"));
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
                ("external_filter_key__bvlS".into(), t("42657469 62655430")),
                ("external_filter_key__0x466c2020".into(), t("true")),
                ("external_filter_key__Drct".into(), t(dir)),
                ("external_filter_key__INcl".into(), t(incl)),
                ("external_filter_key__Brgh".into(), t("100")),
                ("external_filter_key__Shrp".into(), t("25")),
                (
                    "external_filter_key__0x436c7220".into(),
                    t("5247423a203130302e3025203130302e3025203130302e30252028323535203235352032353529 0"),
                ),
                ("external_filter_key__sdwC".into(), t("5247423a20302e302520302e302520302e30252028302030203029 0")),
                ("external_filter_key__BEvl".into(), t(profile)),
                ("external_filter_key__Drkn".into(), t("0")),
            ]));
            if let Some(v) = doc.layers[0].elements_mut(0) {
                v.push(el);
            }
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{rows} rows x {} columns of {cell}px -> {out}", cols.len());
    Ok(())
}
