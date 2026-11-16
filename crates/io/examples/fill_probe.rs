//! Probe document for Color Fill on semi-transparent pixels: one row per blend mode, columns at
//! 100% and 50% opacity: fill_probe <out.png>
//!
//! Content: 256x52 bitmaps, colour (x, 255 - x, 128) across, alpha 5..255 down (rows of 5), so
//! every mode meets faint, half and opaque pixels. Fill colour #c83c1e. Compare Fireworks MX's
//! re-save with cell_compare <file> 280 60.
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_raster::{BlendMode, Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: fill_probe <out.png>")?;
    let mut img = RgbaImage::new(256, 52, Rgba::TRANSPARENT);
    for y in 0..52u32 {
        for x in 0..256u32 {
            let a = (5 + y * 5).min(255) as u8;
            img.set(x as i64, y as i64, Rgba::new(x as u8, (255 - x) as u8, 128, a));
        }
    }
    let (cw, ch) = (280u32, 60u32);
    let opacities = [100, 50];
    let modes = BlendMode::ALL;
    let mut doc = Document::new(cw * opacities.len() as u32, ch * modes.len() as u32, 72.0, None);
    for (row, mode) in modes.into_iter().enumerate() {
        for (col, op) in opacities.into_iter().enumerate() {
            let (x, y) = ((col as u32 * cw + 10) as i64, (row as u32 * ch + 4) as i64);
            let mut el = Element::new(ElementKind::Bitmap(Bitmap {
                x,
                y,
                image: img.clone(),
            }));
            el.name = Some(format!("{} {op}", mode.label()));
            let t = |s: &str| EffectParam::Text(s.into());
            el.effects.push(EffectEntry::new(vec![
                ("mkbFile_WriteOnly_TemporaryEffectUiName".into(), t("Color Fill")),
                ("EffectIsVisible".into(), t("true")),
                ("EffectMoaID".into(), t("{dd54adc0-a279-11d3-b92a000502f3fdbe}")),
                ("Color".into(), t("#c83c1e")),
                ("Opacity".into(), t(&op.to_string())),
                ("Blendmode".into(), t(&mode.fw_code().to_string())),
            ]));
            if let Some(v) = doc.layers[0].elements_mut(0) {
                v.push(el);
            }
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    let labels: Vec<_> = modes.iter().map(|m| m.label()).collect();
    println!("rows: {}", labels.join(","));
    Ok(())
}
