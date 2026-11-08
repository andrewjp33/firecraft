//! Write a Fireworks PNG with Pattern and Web Dither fills, to open in Fireworks MX.
//! `fill_probe PATTERN_IMAGE OUT.png`

use firecraft_doc::Document;
use firecraft_engine::{DocState, PATTERN, ToolOptions, WEB_DITHER, new_pattern};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let image = firecraft_io::textures::decode_image(&std::fs::read(&a[1])?)?;
    let mut d = DocState::new(Document::new(400, 300, 72.0, None));
    let mut o = ToolOptions {
        fill: Some(Rgba::new(0x12, 0x34, 0x56, 255)),
        stroke: None,
        fill_pattern: Some(new_pattern("Berber Rug", image, (0.0, 0.0, 1.0, 1.0))),
        ..ToolOptions::default()
    };
    d.add_shape((20.0, 20.0, 190.0, 130.0), false, &o);
    d.set_fill_category(PATTERN, &o);
    d.add_shape((210.0, 20.0, 380.0, 130.0), false, &o);
    d.set_fill_category(WEB_DITHER, &o);
    d.add_shape((20.0, 150.0, 190.0, 280.0), false, &o);
    d.set_fill_category(WEB_DITHER, &o);
    d.set_web_dither_transparent(true);
    o.fill = Some(Rgba::new(0xa0, 0xb0, 0xc0, 255));
    d.add_shape((210.0, 150.0, 380.0, 280.0), true, &o);
    d.set_fill_category(PATTERN, &o);
    d.set_fill_edge(true, 12, false);
    std::fs::write(&a[2], firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
