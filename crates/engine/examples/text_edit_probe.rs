//! Edit every top-level text object (append `SUFFIX`) so FireCraft draws and saves it, for checking
//! files in Fireworks MX: `text_edit_probe IN.png OUT.png SUFFIX`
use firecraft_doc::ElementKind;
use firecraft_engine::text;
use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let (input, out, suffix) = (&a[1], &a[2], a.get(3).cloned().unwrap_or_default());
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(input)?, None)?);
    for layer in 0..d.doc.layers.len() {
        let n = d.doc.layers[layer].elements(0).len();
        for i in 0..n {
            let r = ElementRef::new(layer, i);
            if !matches!(d.element(r).map(|e| &e.kind), Some(ElementKind::Text(_))) {
                continue;
            }
            d.begin_live_edit("Edit Text");
            d.change_text(r, |t| {
                let n = text::char_count(t);
                text::insert(t, n, &suffix, None);
            });
            d.commit_live_edit();
        }
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
