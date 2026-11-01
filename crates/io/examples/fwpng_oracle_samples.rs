//! Writes Fireworks PNGs for checking against Fireworks MX (tools/fwmx-oracle):
//!   new.png      a new document: bitmap + filled vector triangle + stroked open path
//!   resaved.png  `input` opened and saved again by FireCraft
//! Usage: fwpng_oracle_samples <out-dir> [input.png]
use firecraft_doc::{BezierPoint, Contour, Document, Element, ElementKind, Fill, Path, Stroke};
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().ok_or("out dir")?);
    let mut img = RgbaImage::new(160, 100, Rgba::new(255, 255, 255, 255));
    for y in 20..60 {
        for x in 20..140 {
            img.set(x, y, Rgba::new(255, 200, 0, if x < 80 { 255 } else { 128 }));
        }
    }
    let mut doc = Document::new(300, 200, 72.0, Some(Rgba::WHITE));
    let cell = doc.layers[0].elements_mut(0).ok_or("cell")?;
    cell.push(Element::new(ElementKind::Bitmap(firecraft_doc::Bitmap { x: 10, y: 10, image: img })));
    cell.push(Element::new(ElementKind::Path(Path {
        contours: vec![Contour {
            closed: true,
            points: vec![BezierPoint::corner(200.0, 30.0), BezierPoint::corner(280.0, 30.0), BezierPoint::corner(240.0, 110.0)],
        }],
        fill: Some(Fill { color: Rgba::new(0, 102, 153, 255), category: "fc_Solid".into(), anti_alias: true, gradient: None }),
        stroke: None,
    })));
    cell.push(Element::new(ElementKind::Path(Path {
        contours: vec![Contour {
            closed: false,
            points: vec![
                BezierPoint { x: 20.0, y: 170.0, pred: (0.0, 0.0), succ: (60.0, -60.0) },
                BezierPoint { x: 280.0, y: 170.0, pred: (-60.0, -60.0), succ: (0.0, 0.0) },
            ],
        }],
        fill: None,
        stroke: Some(Stroke { color: Rgba::new(200, 0, 0, 255), category: "bc_Pencil".into(), name: "bn_1-Pixel Anti-Aliased".into(), diameter: 1.0, softness: 0 }),
    })));
    std::fs::write(out.join("new.png"), firecraft_io::save_fireworks_png(&doc)?)?;
    std::fs::write(out.join("new_flat_expected.png"), firecraft_io::export_png(&doc)?)?;
    if let Some(input) = args.next() {
        let bytes = std::fs::read(&input)?;
        let doc = firecraft_io::open(&bytes, Some(input))?;
        std::fs::write(out.join("resaved.png"), firecraft_io::save_fireworks_png(&doc)?)?;
    }
    Ok(())
}
