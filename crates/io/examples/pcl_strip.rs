//! Render a document with every path's stored polyline (`PCLv`) dropped, so strokes follow
//! FireCraft's own flattening: pcl_strip IN.png OUT.png
use firecraft_doc::{Element, ElementKind};

fn strip(e: &mut Element) {
    match &mut e.kind {
        ElementKind::Path(p) => p.pcl = None,
        ElementKind::Group(c) => c.iter_mut().for_each(strip),
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut doc = firecraft_io::open(&std::fs::read(&a[0])?, None)?;
    for l in &mut doc.layers {
        if let Some(cell) = l.elements_mut(0) {
            cell.iter_mut().for_each(strip);
        }
    }
    std::fs::write(&a[1], firecraft_io::encode_png(&doc.render(0), doc.resolution)?)?;
    Ok(())
}
