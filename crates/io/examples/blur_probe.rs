//! Probe document for Fireworks' Gaussian Blur at large radii: one 120x80 colour-ramp bitmap
//! per radius, on a transparent canvas: blur_probe <out.png>
use firecraft_doc::{Bitmap, Document, EffectEntry, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

pub const RADII: [f32; 10] = [50.0, 70.0, 100.0, 150.0, 199.0, 200.0, 201.0, 220.0, 250.0, 30.0];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: blur_probe <out.png>")?;
    let (cw, ch) = (220u32, 160u32);
    let mut doc = Document::new(cw * 5, ch * 2, 72.0, None);
    for (i, r) in RADII.iter().enumerate() {
        let mut img = RgbaImage::new(120, 80, Rgba::TRANSPARENT);
        for y in 0..80 {
            for x in 0..120 {
                let c = Rgba::new(
                    (x * 255 / 119) as u8,
                    (y * 255 / 79) as u8,
                    if (x / 20 + y / 20) % 2 == 0 { 40 } else { 220 },
                    255,
                );
                img.set(x, y, c);
            }
        }
        let (x, y) = ((i as u32 % 5 * cw + 50) as i64, (i as u32 / 5 * ch + 40) as i64);
        let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img }));
        el.name = Some(format!("blur {r}"));
        el.effects.push(EffectEntry::from_pairs(&[
            ("EffectIsVisible", "true"),
            ("EffectMoaID", "{d04ef8c0-71b3-11d1-8c8200a024cdc039}"),
            ("name", "Gaussian Blur..."),
            ("MB_filter_preview_tile_size", "-1 -1"),
            ("mkbFile_WriteOnly_TemporaryEffectUiName", "Gaussian Blur..."),
            ("gaussian_blur_radius", &format!("{r}")),
        ]));
        if let Some(v) = doc.layers[0].elements_mut(0) {
            v.push(el);
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{} objects -> {out}", RADII.len());
    Ok(())
}
