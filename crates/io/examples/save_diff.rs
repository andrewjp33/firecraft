//! Debug helper: open a file, save it with FireCraft, re-open, and list objects whose
//! kind/visibility/blend/opacity/bounds differ.
use firecraft_doc::{Element, ElementKind};

fn walk(els: &[Element], depth: usize, out: &mut Vec<String>) {
    for e in els {
        let kind = match &e.kind {
            ElementKind::Bitmap(_) => "Bitmap",
            ElementKind::Path(_) => "Path",
            ElementKind::Text(_) => "Text",
            ElementKind::Group(_) => "Group",
            ElementKind::Masked { .. } => "Masked",
            ElementKind::Instance { .. } => "Instance",
            ElementKind::Other { .. } => "Other",
        };
        let fill = match &e.kind {
            ElementKind::Path(p) => format!(
                "{:?}",
                p.fill.as_ref().map(|f| (
                    &f.category,
                    f.color,
                    f.gradient
                        .as_ref()
                        .map(|g| (g.start, g.end, g.side, g.colors.len(), g.opacity.len()))
                ))
            ),
            _ => String::new(),
        };
        out.push(format!(
            "{}{kind} vis={} blend={:?} op={} b={:?} {fill}",
            "  ".repeat(depth),
            e.visible,
            e.blend,
            e.opacity,
            e.bounds()
                .map(|b| (b.0.round(), b.1.round(), b.2.round(), b.3.round()))
        ));
        match &e.kind {
            ElementKind::Group(c) => walk(c, depth + 1, out),
            ElementKind::Masked { content, mask, .. } => {
                walk(content, depth + 1, out);
                walk(std::slice::from_ref(mask), depth + 1, out);
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("file")?;
    let a = firecraft_io::open(&std::fs::read(&path)?, None)?;
    let b = firecraft_io::open(&firecraft_io::save_fireworks_png(&a)?, None)?;
    let (mut la, mut lb) = (Vec::new(), Vec::new());
    for l in &a.layers {
        walk(l.elements(0), 0, &mut la);
    }
    for l in &b.layers {
        walk(l.elements(0), 0, &mut lb);
    }
    for (i, (x, y)) in la.iter().zip(&lb).enumerate() {
        if x != y {
            println!("#{i}\n  before: {x}\n  after:  {y}");
        }
    }
    println!(
        "{} vs {} entries; render equal: {}",
        la.len(),
        lb.len(),
        a.render(0) == b.render(0)
    );
    Ok(())
}
