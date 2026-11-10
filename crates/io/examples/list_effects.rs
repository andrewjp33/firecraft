//! List each top-level object's effects, opacity and alpha make-up: list_effects <file.png>
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: list_effects <file.png>")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, Some(path.clone()))?;
    for (li, l) in doc.layers.iter().enumerate() {
        for (ei, e) in l.elements(0).iter().enumerate() {
            if e.effects.is_empty() {
                continue;
            }
            let alpha = match &e.kind {
                ElementKind::Bitmap(b)
                | ElementKind::Text(firecraft_doc::Text {
                    cached: Some(b), ..
                }) => {
                    let px = b.image.pixels();
                    let partial = px.iter().filter(|p| p.a > 0 && p.a < 255).count();
                    let clear = px.iter().filter(|p| p.a == 0).count();
                    if let Ok(dir) = std::env::var("DUMP_DIR") {
                        let png = firecraft_io::encode_png(&b.image, 72.0).unwrap_or_default();
                        std::fs::write(format!("{dir}/obj{li}_{ei}.png"), png).unwrap_or_default();
                    }
                    let faint = px.iter().filter(|p| p.a > 0 && p.a < 8).count();
                    format!("{} px: {partial} partial ({faint} below 8), {clear} clear", px.len())
                }
                _ => String::new(),
            };
            println!(
                "layer {li} #{ei} {} opacity {} {:?} {alpha}",
                e.label(),
                e.opacity,
                e.bounds()
            );
            for fx in &e.effects {
                println!("    {fx:?}");
            }
        }
    }
    Ok(())
}
