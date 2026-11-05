//! Textured strokes for checking against Fireworks MX (tools/fwmx-oracle): one row per case.
//!   texture_probe <out-dir> <file with a Grain texture, e.g. SOLAR.png>
//! Writes probe.png (Fireworks PNG) and probe_fc.png (FireCraft's render).
use firecraft_doc::brush_presets::{self, BrushFields};
use firecraft_doc::{BezierPoint, Contour, Document, Element, ElementKind, Path, Stroke, Texture};
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
    // Stripes under Fireworks' name: shows whether Fireworks uses the file's mask or its own.
    let stripes = Texture::new(
        "Grain",
        16,
        16,
        (0..256).map(|i| if (i % 16) < 8 { 255 } else { 0 }).collect(),
    )
    .ok_or("stripes")?;
    // (category, name, texture amount, edge texture, texture, origin shift)
    let rows: Vec<(&str, &str, Option<i32>, Option<i32>, &Texture, (f64, f64))> = vec![
        (
            "bc_Basic",
            "bn_Hard Rounded",
            Some(800),
            Some(0),
            &grain,
            (0.0, 0.0),
        ),
        ("bc_Air Brush", "bn_Textured", None, None, &grain, (0.0, 0.0)),
        ("bc_Charcoal", "bn_Textured", None, None, &grain, (0.0, 0.0)),
        ("bc_Crayon", "bn_Basic", None, None, &grain, (0.0, 0.0)),
        ("bc_Watercolor", "bn_Heavy", None, None, &grain, (0.0, 0.0)),
        (
            "bc_Basic",
            "bn_Hard Rounded",
            Some(0),
            Some(800),
            &grain,
            (0.0, 0.0),
        ),
        (
            "bc_Basic",
            "bn_Hard Rounded",
            Some(800),
            Some(0),
            &stripes,
            (0.0, 0.0),
        ),
        (
            "bc_Basic",
            "bn_Hard Rounded",
            Some(800),
            Some(0),
            &stripes,
            (5.0, 0.0),
        ),
        ("bc_Watercolor", "bn_Heavy", Some(0), Some(0), &grain, (0.0, 0.0)),
        ("bc_Watercolor", "bn_Thin", None, None, &grain, (0.0, 0.0)),
        ("bc_Oil", "bn_Bristle", None, None, &grain, (0.0, 0.0)),
        ("bc_Random", "bn_Squares", None, None, &grain, (0.0, 0.0)),
    ];
    let (w, row_h) = (520u32, 50.0);
    let mut doc = Document::new(
        w,
        (rows.len() as f64 * row_h + 20.0) as u32,
        72.0,
        Some(Rgba::WHITE),
    );
    let cell = doc.layers[0].elements_mut(0).ok_or("cell")?;
    for (k, (cat, name, btb, bte, tex, shift)) in rows.iter().enumerate() {
        let y = 35.0 + k as f64 * row_h;
        let index = brush_presets::find(cat, name).ok_or("preset")?;
        let mut stroke = Stroke::from_preset(Rgba::BLACK, index, 30.0);
        let base = stroke.brush.clone().ok_or("brush")?;
        let mut f = BrushFields::from_node(&base);
        if let Some(v) = btb {
            f.texture_blend = *v;
        }
        if let Some(v) = bte {
            f.texture_edge = *v;
        }
        stroke.brush = Some(f.to_node(&base));
        stroke.texture = Some((*tex).clone());
        let (x0, x1) = (40.0, w as f64 - 40.0);
        cell.push(Element::new(ElementKind::Path(Path {
            contours: vec![Contour {
                points: vec![
                    BezierPoint {
                        x: x0,
                        y,
                        pred: (0.0, 0.0),
                        succ: (150.0, -12.0),
                    },
                    BezierPoint {
                        x: x1,
                        y,
                        pred: (-150.0, 12.0),
                        succ: (0.0, 0.0),
                    },
                ],
                closed: false,
            }],
            stroke: Some(stroke),
            texture_origin: Some((x0 - 15.0 + shift.0, y - 15.0 + shift.1)),
            ..Path::default()
        })));
    }
    std::fs::write(out.join("probe.png"), firecraft_io::save_fireworks_png(&doc)?)?;
    std::fs::write(out.join("probe_fc.png"), firecraft_io::export_png(&doc)?)?;
    Ok(())
}
