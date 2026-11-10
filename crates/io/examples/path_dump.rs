//! The first path's record (without point lists and tiles): path_dump FILE.png
use firecraft_doc::{Element, ElementKind};

fn print_node(n: &firecraft_fwpng::Node, depth: usize) {
    let tag = String::from_utf8_lossy(&n.tag).into_owned();
    println!("{}{} {}", " ".repeat(depth), tag, n.text);
    if ["PBL", "PCL", "TIL", "MSK"].iter().any(|t| tag.starts_with(t)) {
        return;
    }
    for c in &n.children {
        print_node(c, depth + 1);
    }
}

fn first(e: &Element) -> Option<&Element> {
    match &e.kind {
        ElementKind::Path(_) => Some(e),
        ElementKind::Group(c) => c.iter().find_map(first),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    let e = doc.layers[0]
        .elements(0)
        .iter()
        .find_map(first)
        .ok_or("no path")?;
    if let ElementKind::Path(p) = &e.kind {
        println!(
            "fill {:?}",
            p.fill
                .as_ref()
                .map(|f| (&f.category, f.anti_alias, f.feather, f.color))
        );
        println!(
            "stroke {:?}",
            p.stroke.as_ref().map(|s| (
                &s.category,
                &s.name,
                s.diameter,
                s.softness,
                s.anti_alias,
                s.placement,
                s.color
            ))
        );
    }
    if let Some(r) = &e.raw {
        print_node(r, 0);
    }
    Ok(())
}
