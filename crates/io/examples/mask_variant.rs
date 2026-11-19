//! Variants of a masked object for checking how Fireworks draws it:
//! mask_variant <in> <out> <index> <uniform-mask|white-content|small-mask> (first layer).
use firecraft_doc::ElementKind;
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, out, index, how] = a.as_slice() else { return Err("usage".into()) };
    let mut doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    let i: usize = index.parse()?;
    let el = doc.layers[0].cells[0].get_mut(i).ok_or("index")?;
    let ElementKind::Masked { mask, content, .. } = &mut el.kind else { return Err("not masked".into()) };
    match how.as_str() {
        "uniform-mask" => {
            if let ElementKind::Bitmap(b) = &mut mask.kind {
                b.image = RgbaImage::new(b.image.width(), b.image.height(), Rgba::new(133, 133, 133, 255));
            }
        }
        "white-content" => {
            if let Some(ElementKind::Bitmap(b)) = content.first_mut().map(|c| &mut c.kind) {
                b.image = RgbaImage::new(b.image.width(), b.image.height(), Rgba::WHITE);
            }
        }
        "small-mask" => {
            if let ElementKind::Bitmap(b) = &mut mask.kind {
                b.image = RgbaImage::new(100, 100, Rgba::new(133, 133, 133, 255));
                b.x = 50;
                b.y = 250;
            }
        }
        _ => return Err("how".into()),
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&doc)?)?;
    Ok(())
}
