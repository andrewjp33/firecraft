//! Write Fireworks PNGs with text made and edited here, to open in Fireworks MX.
//! `text_probe MX_MADE.png NEW_OUT.png EDITED_OUT.png`

use firecraft_doc::Document;
use firecraft_engine::text::{self, new_text};
use firecraft_engine::{DocState, ToolOptions};
use firecraft_raster::Rgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut d = DocState::new(Document::new(400, 300, 72.0, None));
    let mut o = ToolOptions {
        fill: Some(Rgba::new(200, 30, 30, 255)),
        ..ToolOptions::default()
    };
    o.text_style.font = "DejaVu Sans".into();
    o.text_style.size = 24.0;
    let r = d.start_new_text(new_text((20.0, 20.0, 21.0, 44.0), true, &o));
    d.change_text(r, |t| text::insert(t, 0, "Made in FireCraft\rSecond line", None));
    d.change_text(r, |t| text::format(t, 8..17, &|run| run.bold = true));
    d.commit_live_edit();
    o.text_style.size = 14.0;
    o.text_style.align = 1;
    let r = d.start_new_text(new_text((20.0, 120.0, 220.0, 134.0), false, &o));
    d.change_text(r, |t| {
        text::insert(t, 0, "Centred in a fixed block that has to wrap", None)
    });
    d.commit_live_edit();
    std::fs::write(&a[2], firecraft_io::save_fireworks_png(&d.doc)?)?;

    let mut m = DocState::new(firecraft_io::open(&std::fs::read(&a[1])?, None)?);
    let r = firecraft_engine::ElementRef::new(0, 2);
    m.begin_live_edit("Edit Text");
    m.change_text(r, |t| {
        let n = text::char_count(t);
        text::insert(t, n, " edited", None);
    });
    m.commit_live_edit();
    m.doc.layers[0].elements_mut(0).ok_or("layer")?[0].translate(0.0, 12.0);
    std::fs::write(&a[3], firecraft_io::save_fireworks_png(&m.doc)?)?;
    Ok(())
}
