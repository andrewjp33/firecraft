//! Build a document with one shape per effect in the Property inspector's "+" menu (each with
//! Fireworks' defaults), for checking that Fireworks MX opens FireCraft's effects:
//! effects_doc <out.png>
use firecraft_doc::{Document, Element};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: effects_doc <out.png>")?;
    let names: Vec<(&str, &str)> = firecraft_ui_egui::effects_ui::catalogue().collect();
    let cols = 5usize;
    let rows = names.len().div_ceil(cols);
    let mut doc = Document::new(
        (cols * 140) as u32,
        (rows * 140) as u32,
        72.0,
        Some(Rgba::new(232, 232, 232, 255)),
    );
    let mut engine = firecraft_engine::DocState::new(doc.clone());
    let opts = firecraft_engine::ToolOptions {
        fill: Some(Rgba::new(61, 127, 196, 255)),
        stroke: None,
        ..Default::default()
    };
    for (i, (_, name)) in names.iter().enumerate() {
        let (x, y) = ((i % cols * 140 + 35) as f64, (i / cols * 140 + 35) as f64);
        engine.add_shape((x, y, x + 70.0, y + 70.0), i % 2 == 0, &opts);
        let fx = firecraft_ui_egui::effects_ui::new_effect(name).ok_or("unknown effect")?;
        engine.set_effects(vec![fx], "Add Effect");
        if let Some(r) = engine.selection.first().copied() {
            if let Some(e) = engine
                .doc
                .layers
                .get_mut(r.layer)
                .and_then(|l| l.elements_mut(0))
                .and_then(|v| v.get_mut(r.index))
            {
                e.name = Some(name.to_string());
            }
        }
    }
    doc = engine.doc.clone();
    let _: &Element = &doc.layers[0].elements(0)[0];
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{} shapes -> {out}", names.len());
    Ok(())
}
