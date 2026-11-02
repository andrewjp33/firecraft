//! List a document's top-level objects (kind, name, bounds) and its symbols: list_objects <file.png>
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: list_objects <file.png>")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for (li, l) in doc.layers.iter().enumerate() {
        for (ei, e) in l.elements(0).iter().enumerate() {
            let kind = match &e.kind {
                ElementKind::Instance {
                    symbol, name, matrix, ..
                } => format!(
                    "Instance of {name} ({symbol:x}) matrix a={:.3} b={:.3} c={:.3} d={:.3} e={:.1} f={:.1}",
                    matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f
                ),
                k => format!("{:?}", std::mem::discriminant(k)),
            };
            println!(
                "layer {li} '{}' #{ei} {} {kind} {:?}",
                l.name,
                e.label(),
                e.bounds()
            );
        }
    }
    for s in doc.symbols.iter() {
        println!(
            "symbol {:x} '{}' {:?} frames {} content {:?}",
            s.id,
            s.name,
            s.kind,
            s.frames,
            s.content_bounds()
        );
        for l in &s.layers {
            for e in l.elements(0) {
                println!("    {} {:?}", e.label(), e.bounds());
            }
        }
    }
    Ok(())
}
