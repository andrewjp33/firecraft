//! A document with FireCraft-made symbols and instances, to check Fireworks MX opens them:
//! symbols_doc <out.png>
use firecraft_doc::{Document, SymbolKind};
use firecraft_engine::{DocState, ElementRef, ToolOptions};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: symbols_doc <out.png>")?;
    let mut d = DocState::new(Document::new(320, 200, 72.0, Some(Rgba::WHITE)));
    let opts = |c| ToolOptions {
        fill: Some(c),
        stroke: None,
        ..Default::default()
    };
    d.add_shape(
        (10.0, 10.0, 60.0, 50.0),
        false,
        &opts(Rgba::new(220, 60, 40, 255)),
    );
    d.add_shape(
        (20.0, 30.0, 70.0, 80.0),
        true,
        &opts(Rgba::new(40, 120, 220, 255)),
    );
    d.add_shape(
        (200.0, 100.0, 260.0, 130.0),
        false,
        &opts(Rgba::new(40, 180, 90, 255)),
    );
    let r = |index| ElementRef { layer: 0, index };
    d.selection = vec![r(0), r(1)];
    let g = d
        .convert_to_symbol_with("fc_graphic", SymbolKind::Graphic, true)
        .ok_or("convert")?;
    d.selection = vec![r(0)];
    d.convert_to_symbol("fc_anim", SymbolKind::Animation)
        .ok_or("convert")?;
    d.insert_instance(g, 150.0, 150.0);
    d.rename_symbol(g, "fc_graphic_renamed");
    std::fs::write(&out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    println!("{} symbols -> {out}", d.doc.symbols.len());
    Ok(())
}
