//! Each converted text group's character paths: the first point of each (one line per group):
//! `first_nodes FILE.png`
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for e in doc.layers.iter().flat_map(|l| l.elements(0)) {
        let ElementKind::Group(c) = &e.kind else { continue };
        let xs: Vec<String> = c
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Path(p) => p
                    .contours
                    .first()?
                    .points
                    .first()
                    .map(|q| format!("{:.2},{:.2}", q.x, q.y)),
                _ => None,
            })
            .collect();
        println!("{}", xs.join(" "));
    }
    Ok(())
}
