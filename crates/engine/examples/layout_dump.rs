//! Each character's position and advance as FireCraft lays out a file's text objects:
//! `layout_dump FILE.png`
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for e in doc.layers.iter().flat_map(|l| l.elements(0)) {
        let ElementKind::Text(t) = &e.kind else { continue };
        let lay = firecraft_engine::text::layout_of(t);
        let row: Vec<String> = lay
            .glyphs
            .iter()
            .map(|g| {
                format!(
                    "{}{:.2}+{:.2}[e{:.2} f{:.2}]",
                    g.ch, g.x, g.advance, g.transform.e, g.transform.f
                )
            })
            .collect();
        println!("{}", row.join(" "));
    }
    Ok(())
}
