//! Compositing cases for measuring Fireworks MX's rounding (tools/fwmx-oracle):
//!   composite_probe <out-dir>      writes probe.png (Fireworks PNG) and probe_fc.png
//! Canvas (100, 150, 200). Rows: A bitmap, alpha = column (0..255), colour (200, 40, 90);
//! B the same at 50% object opacity; C 100 squares (2 x 8 px) of solid (200, 40, 90) at object
//! opacity 1..100%; D bitmap A over a 50%-alpha white bitmap (onto a semi-transparent pixel,
//! the whole stack over the canvas).
use firecraft_doc::{BezierPoint, Bitmap, Contour, Document, Element, ElementKind, Fill, Path};
use firecraft_raster::{Rgba, RgbaImage};

fn ramp(c: Rgba) -> RgbaImage {
    let mut img = RgbaImage::new(256, 8, Rgba::TRANSPARENT);
    for y in 0..8 {
        for x in 0..256 {
            img.set(x, y, Rgba { a: x as u8, ..c });
        }
    }
    img
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("out dir")?);
    let col = Rgba::new(200, 40, 90, 255);
    let mut doc = Document::new(280, 60, 72.0, Some(Rgba::new(100, 150, 200, 255)));
    let cell = doc.layers[0].elements_mut(0).ok_or("cell")?;
    cell.push(Element::new(ElementKind::Bitmap(Bitmap {
        x: 10,
        y: 2,
        image: ramp(col),
    })));
    let mut b = Element::new(ElementKind::Bitmap(Bitmap {
        x: 10,
        y: 12,
        image: ramp(col),
    }));
    b.opacity = 50.0;
    cell.push(b);
    for k in 0..100 {
        let x = 10.0 + k as f64 * 2.0;
        let mut e = Element::new(ElementKind::Path(Path {
            contours: vec![Contour {
                points: vec![
                    BezierPoint::corner(x, 22.0),
                    BezierPoint::corner(x + 2.0, 22.0),
                    BezierPoint::corner(x + 2.0, 30.0),
                    BezierPoint::corner(x, 30.0),
                ],
                closed: true,
            }],
            fill: Some(Fill {
                color: col,
                category: "fc_Solid".into(),
                anti_alias: false,
                feather: 0,
                pattern: None,
                dither: None,
                dither_transparent: false,
                gradient: None,
                texture: None,
                texture_amount: 0,
                texture_transparent: false,
            }),
            ..Path::default()
        }));
        e.opacity = (k + 1) as f32;
        cell.push(e);
    }
    let white = RgbaImage::new(256, 8, Rgba::new(255, 255, 255, 128));
    cell.push(Element::new(ElementKind::Bitmap(Bitmap {
        x: 10,
        y: 34,
        image: white,
    })));
    cell.push(Element::new(ElementKind::Bitmap(Bitmap {
        x: 10,
        y: 34,
        image: ramp(col),
    })));
    std::fs::write(out.join("probe.png"), firecraft_io::save_fireworks_png(&doc)?)?;
    std::fs::write(out.join("probe_fc.png"), firecraft_io::export_png(&doc)?)?;
    Ok(())
}
