//! Write each distinct stroke texture of a Fireworks PNG as a PGM: texture_dump <file.png> <out_dir>
use firecraft_doc::{Element, ElementKind};

fn walk(els: &[Element], out: &mut Vec<firecraft_doc::Texture>) {
    for e in els {
        match &e.kind {
            ElementKind::Path(p) => {
                if let Some(t) = p.stroke.as_ref().and_then(|s| s.texture.clone())
                    && !out.iter().any(|o| o.name == t.name)
                {
                    out.push(t);
                }
            }
            ElementKind::Group(c) => walk(c, out),
            ElementKind::Masked { content, .. } => walk(content, out),
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("file")?)?, None)?;
    let dir = std::path::PathBuf::from(a.get(2).ok_or("out dir")?);
    let mut found = Vec::new();
    for l in &doc.layers {
        for f in &l.cells {
            walk(f, &mut found);
        }
    }
    for t in found {
        let mut pgm = format!("P5\n{} {}\n255\n", t.width, t.height).into_bytes();
        pgm.extend_from_slice(&t.mask);
        std::fs::write(dir.join(format!("{}.pgm", t.name)), pgm)?;
        println!("{} {}x{}", t.name, t.width, t.height);
    }
    Ok(())
}
