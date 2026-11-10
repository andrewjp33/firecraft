//! Render a document with Fireworks MX's perspective mode on and off: `perspective_modes IN.png
//! OUT_PREFIX [all]` writes OUT_PREFIX_mx.png and OUT_PREFIX_fc.png (with `all`, every frame as
//! OUT_PREFIX_{mx,fc}_fN.png) and lists instances with a perspective matrix.
use firecraft_doc::{Element, ElementKind};

fn walk(e: &Element, n: &mut usize) {
    match &e.kind {
        ElementKind::Instance { matrix, symbol, .. } if !matrix.is_affine() => {
            *n += 1;
            println!("perspective instance of symbol {symbol:?}, matrix {matrix:?}");
        }
        ElementKind::Group(c) => c.iter().for_each(|x| walk(x, n)),
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = firecraft_io::open(&std::fs::read(&a[1])?, None)?;
    let mut n = 0;
    for l in &doc.layers {
        l.elements(0).iter().for_each(|e| walk(e, &mut n));
    }
    println!(
        "perspective instances in frame 1: {n}; file's mode: mx = {}",
        doc.mx_perspective
    );
    let frames = doc.frames.len().max(1);
    let all = a.get(3).is_some_and(|v| v == "all");
    for (mx, tag) in [(true, "mx"), (false, "fc")] {
        doc.mx_perspective = mx;
        std::fs::write(
            format!("{}_{tag}.png", a[2]),
            firecraft_io::encode_png(&doc.render(0), doc.resolution)?,
        )?;
        if all {
            for f in 0..frames {
                let img = firecraft_io::encode_png(&doc.render(f), doc.resolution)?;
                std::fs::write(format!("{}_{tag}_f{f}.png", a[2]), img)?;
            }
        }
    }
    Ok(())
}
