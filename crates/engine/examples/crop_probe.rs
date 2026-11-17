//! Probe document for the Crop tool, to crop in Fireworks MX and compare: crop_probe <out.png>
//!
//! 300x200 white canvas. A 120x90 bitmap at (150, 90) with varied colours and an oval of soft
//! alpha (it crosses the right and bottom of a crop to (40, 30)-(250, 160)); a rectangle across
//! the crop's top-left corner; an ellipse wholly outside it (top right); a rectangle wholly
//! inside; a bitmap wholly outside (bottom left).
use firecraft_doc::{Bitmap, Document, Element, ElementKind};
use firecraft_engine::{DocState, ToolOptions};
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: crop_probe <out.png>")?;
    let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
    let opts = |c| ToolOptions {
        fill: Some(c),
        stroke: None,
        ..Default::default()
    };
    d.add_shape((10.0, 10.0, 90.0, 70.0), false, &opts(Rgba::new(220, 60, 40, 255)));
    d.add_shape((255.0, 5.0, 295.0, 45.0), true, &opts(Rgba::new(40, 120, 220, 255)));
    d.add_shape((100.0, 50.0, 140.0, 80.0), false, &opts(Rgba::new(40, 180, 90, 255)));
    let bitmap = |w: u32, h: u32, x: i64, y: i64| {
        let mut img = RgbaImage::new(w, h, Rgba::TRANSPARENT);
        for py in 0..h {
            for px in 0..w {
                let (fx, fy) = (px as f64 / w as f64 - 0.5, py as f64 / h as f64 - 0.5);
                let r = (fx * fx + fy * fy).sqrt() * 2.0;
                let a = ((1.15 - r) * 400.0).clamp(0.0, 255.0) as u8;
                let c = Rgba::new((px * 255 / w) as u8, (py * 255 / h) as u8, ((px + py) * 3 % 256) as u8, a);
                img.set(i64::from(px), i64::from(py), c);
            }
        }
        Element::new(ElementKind::Bitmap(Bitmap { x, y, image: img }))
    };
    d.add_element(bitmap(120, 90, 150, 90), "Bitmap");
    d.add_element(bitmap(30, 20, 5, 175), "Bitmap");
    std::fs::write(&out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
