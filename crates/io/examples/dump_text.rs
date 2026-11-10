//! Print each text object's record (without its image tiles and characters): dump_text <file.png>
use firecraft_doc::{Element, ElementKind};

fn print_node(n: &firecraft_fwpng::Node, depth: usize) {
    let tag = String::from_utf8_lossy(&n.tag).into_owned();
    println!("{}{} {}", " ".repeat(depth), tag, n.text);
    if (tag.starts_with("TFS") && std::env::var_os("DUMP_RUNS").is_none())
        || tag.starts_with("TIL")
        || tag.starts_with("DTA")
    {
        return;
    }
    for c in &n.children {
        print_node(c, depth + 1);
    }
}

fn walk(e: &Element) {
    if let ElementKind::Group(c) = &e.kind {
        c.iter().for_each(walk);
    }
    if matches!(e.kind, ElementKind::Text(_))
        && let Some(r) = &e.raw
    {
        print_node(r, 0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: dump_text <file.png>")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for l in &doc.layers {
        l.elements(0).iter().for_each(walk);
    }
    for sym in doc.symbols.iter() {
        for l in &sym.layers {
            l.elements(0).iter().for_each(walk);
        }
    }
    Ok(())
}
