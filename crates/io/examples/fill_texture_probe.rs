//! Textured fills for checking against Fireworks MX (tools/fwmx-oracle): one rectangle per case.
//!   fill_texture_probe <out-dir> <file with a Grain texture, e.g. SOLAR.png>
//! Writes probe.png (Fireworks PNG) and probe_fc.png (FireCraft's render).
use firecraft_doc::{BezierPoint, Contour, Document, Element, ElementKind, Fill, Path, Texture};
use firecraft_raster::Rgba;

fn grain(file: &str) -> Option<Texture> {
    let doc = firecraft_io::open(&std::fs::read(file).ok()?, None).ok()?;
    doc.layers
        .iter()
        .flat_map(|l| l.cells.iter().flatten())
        .find_map(|e| match &e.kind {
            ElementKind::Path(p) => p.stroke.as_ref().and_then(|s| s.texture.clone()),
            _ => None,
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(a.get(1).ok_or("out dir")?);
    let grain = grain(a.get(2).ok_or("grain file")?).ok_or("no texture in file")?;
    let stripes =
        Texture::new("Stripes", 8, 8, (0..64).map(|i| ((i % 8) * 36) as u8).collect()).ok_or("stripes")?;
    // (colour, texture, amount per mille, transparent, origin shift)
    let rows: Vec<(Rgba, &Texture, u16, bool, (f64, f64))> = vec![
        (Rgba::new(200, 30, 30, 255), &grain, 500, false, (0.0, 0.0)),
        (Rgba::new(200, 30, 30, 255), &grain, 1000, false, (0.0, 0.0)),
        (Rgba::new(200, 30, 30, 255), &grain, 500, true, (0.0, 0.0)),
        (Rgba::new(200, 30, 30, 255), &grain, 1000, true, (0.0, 0.0)),
        (Rgba::new(20, 60, 200, 255), &stripes, 700, false, (0.0, 0.0)),
        (Rgba::new(20, 60, 200, 255), &stripes, 700, false, (3.0, 2.0)),
        (Rgba::new(20, 60, 200, 255), &stripes, 700, true, (0.0, 0.0)),
        (Rgba::new(20, 160, 60, 128), &grain, 600, true, (0.0, 0.0)),
    ];
    let (w, row_h) = (400u32, 40.0);
    let mut doc = Document::new(
        w,
        (rows.len() as f64 * row_h + 10.0) as u32,
        72.0,
        Some(Rgba::WHITE),
    );
    let cell = doc.layers[0].elements_mut(0).ok_or("cell")?;
    for (k, (color, tex, amount, transparent, shift)) in rows.iter().enumerate() {
        let y = 10.0 + k as f64 * row_h;
        let (x0, x1, y1) = (20.0, w as f64 - 20.0, y + 30.0);
        cell.push(Element::new(ElementKind::Path(Path {
            contours: vec![Contour {
                points: vec![
                    BezierPoint::corner(x0, y),
                    BezierPoint::corner(x1, y),
                    BezierPoint::corner(x1, y1),
                    BezierPoint::corner(x0, y1),
                ],
                closed: true,
            }],
            fill: Some(Fill {
                color: *color,
                category: "fc_Solid".into(),
                anti_alias: true,
                feather: 0,
                pattern: None,
                dither: None,
                dither_transparent: false,
                gradient: None,
                texture: Some((*tex).clone()),
                texture_amount: *amount,
                texture_transparent: *transparent,
            }),
            texture_origin: Some((x0 + shift.0, y + shift.1)),
            ..Path::default()
        })));
    }
    std::fs::write(out.join("probe.png"), firecraft_io::save_fireworks_png(&doc)?)?;
    std::fs::write(out.join("probe_fc.png"), firecraft_io::export_png(&doc)?)?;
    Ok(())
}
