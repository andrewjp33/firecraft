//! Probe an effect stack step by step: stack_probe <source.png> <layer> <element> <out.png>
//!
//! Takes the Live Effects of one bitmap in a Fireworks document and builds a probe with one
//! column per prefix of that stack (no effects, the first one, the first two, ...). Rows: an
//! opaque grey ramp (0..255 left to right) over a colour ramp, and a 200x200 crop of the
//! element's own image. Fit FireCraft to Fireworks MX's re-save with cell_compare.
use firecraft_doc::{Bitmap, Document, Element, ElementKind};
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [src, li, ei, out] = a.as_slice() else {
        return Err("usage: stack_probe <source.png> <layer> <element> <out.png>".into());
    };
    let doc = firecraft_io::open(&std::fs::read(src)?, Some(src.clone()))?;
    let el = &doc.layers[li.parse::<usize>()?].elements(0)[ei.parse::<usize>()?];
    let ElementKind::Bitmap(b) = &el.kind else {
        return Err("not a bitmap".into());
    };
    let mut ramp = RgbaImage::new(256, 64, Rgba::TRANSPARENT);
    for y in 0..64u32 {
        for x in 0..256u32 {
            let v = x as u8;
            let c = if y < 32 {
                Rgba::new(v, v, v, 255)
            } else {
                let t = (y - 32) * 255 / 31;
                Rgba::new(v, (255 - x) as u8, t as u8, 255)
            };
            ramp.set(x as i64, y as i64, c);
        }
    }
    let (cx, cy) = (
        b.image.width().saturating_sub(200) / 2,
        b.image.height().saturating_sub(200) / 2,
    );
    let mut crop = RgbaImage::new(200, 200, Rgba::TRANSPARENT);
    for y in 0..200u32 {
        for x in 0..200u32 {
            crop.set(
                x as i64,
                y as i64,
                b.image
                    .get((cx + x) as i64, (cy + y) as i64)
                    .unwrap_or(Rgba::TRANSPARENT),
            );
        }
    }
    let n = el.effects.len() + 1;
    let (cw, ch) = (280u32, 230u32);
    let mut probe = Document::new(cw * n as u32, ch * 2, 72.0, None);
    for k in 0..n {
        for (row, img) in [&ramp, &crop].into_iter().enumerate() {
            let (x, y) = ((k as u32 * cw + 10) as i64, (row as u32 * ch + 10) as i64);
            let mut e = Element::new(ElementKind::Bitmap(Bitmap {
                x,
                y,
                image: img.clone(),
            }));
            e.name = Some(format!("k{k} r{row}"));
            e.effects = el.effects[..k].to_vec();
            if let Some(v) = probe.layers[0].elements_mut(0) {
                v.push(e);
            }
        }
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&probe)?)?;
    println!("{n} columns of {cw}x{ch} -> {out}");
    Ok(())
}
