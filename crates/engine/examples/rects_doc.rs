//! A document with Rectangle-tool rectangles (sharp, 30% rounded, and a rounded one rotated
//! 30 degrees about its centre), to check Fireworks reads them as rectangle primitives:
//! rects_doc <out.png>
use firecraft_engine::DocState;
use firecraft_engine::ToolOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("out.png")?;
    let doc = firecraft_doc::Document::new(500, 400, 72.0, Some(firecraft_raster::Rgba::WHITE));
    let mut d = DocState::new(doc);
    let mut opts = ToolOptions::default();
    d.add_shape((20.0, 20.0, 180.0, 120.0), false, &opts);
    opts.roundness = 0.3;
    d.add_shape((220.0, 20.0, 460.0, 160.0), false, &opts);
    d.add_shape((100.0, 220.0, 300.0, 340.0), false, &opts);
    d.numeric_transform(1.0, 1.0, 30.0);
    std::fs::write(out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
