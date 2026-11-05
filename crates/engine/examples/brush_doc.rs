//! A document with one line per stroke preset (Fireworks' 54 brush presets, CS's dashed ones
//! included), to check Fireworks opens what FireCraft writes: brush_doc <out.png>
use firecraft_doc::brush_presets::{BRUSH_PRESETS, label};
use firecraft_engine::{DocState, ToolOptions};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("out.png")?;
    let doc = firecraft_doc::Document::new(900, 700, 72.0, Some(Rgba::WHITE));
    let mut d = DocState::new(doc);
    let mut opts = ToolOptions {
        stroke: Some(Rgba::new(20, 60, 160, 255)),
        ..ToolOptions::default()
    };
    for (i, p) in BRUSH_PRESETS.iter().enumerate() {
        opts.stroke_preset = i;
        opts.stroke_size = p.diameter.clamp(1, 20) as u32;
        opts.stroke_softness = (p.softness / 10).clamp(0, 100) as u8;
        let (x, y) = (20.0 + 300.0 * (i % 3) as f64, 20.0 + 36.0 * (i / 3) as f64);
        d.add_line((x, y), (x + 260.0, y + 10.0), &opts);
        println!("{i}: {} / {}", p.category, label(p));
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
