//! Print each path's fill record (`FPLv` and the handles): dump_fill <file.png>
use firecraft_doc::ElementKind;

fn print_node(n: &firecraft_fwpng::Node, depth: usize) {
    println!(
        "{}{} {}",
        " ".repeat(depth),
        String::from_utf8_lossy(&n.tag),
        n.text
    );
    for c in &n.children {
        print_node(c, depth + 1);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: dump_fill <file.png>")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for l in &doc.layers {
        for e in l.elements(0) {
            if let ElementKind::Path(p) = &e.kind {
                println!(
                    "path fill {:?}",
                    p.fill.as_ref().map(|f| (&f.category, f.anti_alias, f.color))
                );
                if let Some(r) = &e.raw {
                    let tags: Vec<String> = r
                        .children
                        .iter()
                        .map(|c| String::from_utf8_lossy(&c.tag).into_owned())
                        .collect();
                    println!("children: {}", tags.join(" "));
                    println!(
                        "PCF={:?} TOX={:?}",
                        r.child("PCF").map(|n| n.text.clone()),
                        r.float_of("TOX")
                    );
                    if let Some(pri) = r.child("PRI") {
                        for t in ["PSX", "PSY", "PEX", "PEY", "PFX", "PFY"] {
                            print!("PRI.{t}={:?} ", pri.float_of(t));
                        }
                        println!();
                    }
                    for t in ["PSX", "PSY", "PEX", "PEY", "PFX", "PFY"] {
                        print!("{t}={:?} ", r.float_of(t));
                    }
                    println!();
                    if let Some(a) = r.child("PAT") {
                        if let Some(f) = a.child("FPL") {
                            print_node(f, 0);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
