//! Debug helper: list what each layer shows in a frame (kinds, visibility, effects).
use firecraft_doc::{Element, ElementKind};
fn show(e: &Element, d: usize) {
    let k = match &e.kind {
        ElementKind::Instance { symbol, .. } => format!("Instance {symbol:x}"),
        ElementKind::Bitmap(b) => {
            let c = b.image.get(b.image.width() as i64 / 2, b.image.height() as i64 / 2);
            format!("Bitmap {}x{} at {},{} center={c:?}", b.image.width(), b.image.height(), b.x, b.y)
        }
        ElementKind::Group(_) => "Group".into(),
        ElementKind::Masked { .. } => "Masked".into(),
        ElementKind::Path(_) => "Path".into(),
        ElementKind::Text(_) => "Text".into(),
        ElementKind::Other { kind, .. } => kind.clone(),
    };
    println!("{}{k} vis={} op={} blend={:?} effects={:?}", "  ".repeat(d), e.visible, e.opacity, e.blend, e.effects);
    match &e.kind {
        ElementKind::Group(c) => c.iter().for_each(|c| show(c, d + 1)),
        ElementKind::Masked { content, mask, mode, .. } => {
            println!("{}mask ({mode:?}):", "  ".repeat(d + 1));
            show(mask, d + 2);
            println!("{}content:", "  ".repeat(d + 1));
            content.iter().for_each(|c| show(c, d + 2));
        }
        _ => {}
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let doc = firecraft_io::open(&std::fs::read(&a[0])?, None)?;
    let f: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1) - 1;
    for (i, l) in doc.layers.iter().enumerate() {
        println!("layer {i} '{}' visible_in={} shared={} cells={}", l.name, l.visible_in(f), l.shared, l.cells.len());
        for e in l.elements(f) { show(e, 1); }
    }
    for s in doc.symbols.iter() {
        println!("symbol {:x} '{}' frames={}", s.id, s.name, s.frames);
        for l in &s.layers { for e in l.elements(0) { show(e, 1); } }
    }
    Ok(())
}
