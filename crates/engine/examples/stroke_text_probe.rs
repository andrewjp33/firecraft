//! Apply each stroke control to every text object in a file and report whether its drawing
//! changes: `stroke_text_probe FILE.png`
use firecraft_doc::ElementKind;
use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(&a[1])?, None)?;
    for (li, l) in doc.layers.iter().enumerate() {
        for (i, e) in l.elements(0).iter().enumerate() {
            let ElementKind::Text(t) = &e.kind else { continue };
            let text: String = t
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
                .chars()
                .take(20)
                .collect();
            println!(
                "layer {li} #{i} {text:?}: stroke {:?}",
                t.stroke.as_ref().map(|s| (s.color, s.diameter, s.name.clone()))
            );
            let tries: [(&str, Box<dyn Fn(&mut DocState)>); 3] = [
                (
                    "colour",
                    Box::new(|d| d.set_stroke_color(Some(firecraft_raster::Rgba::new(255, 0, 0, 255)))),
                ),
                (
                    "size",
                    Box::new(|d| d.edit_stroke("Stroke Size", false, |s| s.diameter += 4.0)),
                ),
                (
                    "preset 0",
                    Box::new(|d| d.set_stroke_preset(0, &Default::default())),
                ),
            ];
            for (name, f) in tries {
                let mut d = DocState::new(doc.clone());
                let before = d.doc.render(0);
                d.selection = vec![ElementRef::new(li, i)];
                f(&mut d);
                let after = d.doc.render(0);
                let changed = before
                    .pixels()
                    .iter()
                    .zip(after.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                let ElementKind::Text(t2) = &d.doc.layers[li].elements(0)[i].kind else {
                    continue;
                };
                println!(
                    "  {name}: {changed} pixels changed; stroke now {:?}",
                    t2.stroke.as_ref().map(|s| (s.color, s.diameter, s.name.clone()))
                );
            }
        }
    }
    Ok(())
}
