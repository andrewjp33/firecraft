//! Copies of a document with every top-level element moved, for testing in Fireworks:
//! `move_variants IN.png OUT_DIR` writes y1.png, y2.png (moved down 1 and 2 pixels) and x1.png.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(&a[1])?, None)?;
    let out = std::path::Path::new(&a[2]);
    for (name, dx, dy) in [("y1", 0.0, 1.0), ("y2", 0.0, 2.0), ("x1", 1.0, 0.0)] {
        let mut d = doc.clone();
        for l in &mut d.layers {
            if let Some(els) = l.elements_mut(0) {
                els.iter_mut().for_each(|e| e.translate(dx, dy));
            }
        }
        std::fs::write(
            out.join(format!("{name}.png")),
            firecraft_io::save_fireworks_png(&d)?,
        )?;
    }
    Ok(())
}
