//! For files of converted two-character texts ("AA", "BB"...): the distance between each group's two
//! character paths, i.e. the character's advance as placed. `pair_advances FILE.png`
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for e in doc.layers.iter().flat_map(|l| l.elements(0)) {
        let ElementKind::Group(c) = &e.kind else { continue };
        let first = |i: usize| match c.get(i).map(|e| &e.kind) {
            Some(ElementKind::Path(p)) => p.contours.first().and_then(|c| c.points.first()).map(|q| q.x),
            _ => None,
        };
        if let (Some(a), Some(b)) = (first(0), first(1)) {
            println!("{:.4}", b - a);
        } else {
            println!("-");
        }
    }
    Ok(())
}
