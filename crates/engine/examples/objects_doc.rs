//! A document exercising the object commands (group, rotate, flip, align, rename), to check that
//! Fireworks MX opens the result: objects_doc <out.png>
use firecraft_doc::Document;
use firecraft_engine::{Align, DocState, ElementRef, ToolOptions, Turn};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: objects_doc <out.png>")?;
    let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
    let opts = |c| ToolOptions {
        fill: Some(c),
        stroke: None,
        ..Default::default()
    };
    d.add_shape(
        (20.0, 20.0, 80.0, 60.0),
        false,
        &opts(Rgba::new(220, 60, 40, 255)),
    );
    d.add_shape(
        (100.0, 40.0, 150.0, 120.0),
        true,
        &opts(Rgba::new(40, 120, 220, 255)),
    );
    d.add_shape(
        (180.0, 30.0, 260.0, 70.0),
        false,
        &opts(Rgba::new(40, 180, 90, 255)),
    );
    let r = |index| ElementRef::new(0, index);
    d.selection = vec![r(2)];
    d.turn(Turn::Rotate90Cw);
    d.turn(Turn::FlipHorizontal);
    d.rename_object(r(2), "turned");
    d.selection = vec![r(0), r(1)];
    d.align(Align::Top);
    d.group();
    let doc = &d.doc;
    std::fs::write(&out, firecraft_io::save_fireworks_png(doc)?)?;
    println!(
        "{} top-level objects -> {out}",
        doc.layers.iter().map(|l| l.elements(0).len()).sum::<usize>()
    );
    Ok(())
}
