//! Print a Fireworks PNG's document tree, one node per line: dump_tree <file.png> [filter]
//! (with a filter, only lines containing it, with their path).
use firecraft_fwpng::{FwPng, Kind, Node};

fn walk(n: &Node, path: &str, filter: Option<&str>) {
    let k = match n.kind {
        Kind::Vector => 'v',
        Kind::Int => 'i',
        Kind::Float => 'f',
        Kind::Bool => 'b',
        Kind::Str => 's',
    };
    let tag = String::from_utf8_lossy(&n.tag);
    let here = format!("{path}/{tag}{k}");
    let line = if n.text.is_empty() {
        here.clone()
    } else {
        format!("{here} = {}", n.text)
    };
    if filter.is_none_or(|f| line.contains(f)) {
        println!("{line}");
    }
    for c in &n.children {
        walk(c, &here, filter);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let png = FwPng::read(&std::fs::read(
        a.get(1).ok_or("usage: dump_tree <file.png> [filter]")?,
    )?)?;
    let tree = png.tree()?.ok_or("not a Fireworks PNG")?;
    walk(&tree, "", a.get(2).map(String::as_str));
    Ok(())
}
