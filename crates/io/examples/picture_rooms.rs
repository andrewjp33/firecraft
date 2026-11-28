//! How big a copy of each object is, as Fireworks MX sizes its clipboard picture:
//! picture_rooms FILE... prints, per object (top first in each layer), its bounds, the picture
//! (`Element::picture_bounds` rounded out) and the room it adds on each side, as
//! tools/fwmx-oracle's measurements (copy, File > New, paste) report them.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const EPS: f64 = 1e-6;
    for path in std::env::args().skip(1) {
        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned());
        let doc = firecraft_io::open(&std::fs::read(&path)?, name.clone())?;
        let ctx = firecraft_doc::RenderCtx {
            symbols: &doc.symbols,
            math: doc.color_math,
            mx_perspective: doc.mx_perspective,
            ..firecraft_doc::RenderCtx::EMPTY
        };
        for (li, layer) in doc.layers.iter().enumerate().filter(|(_, l)| !l.web) {
            for (i, el) in layer.elements(0).iter().rev().enumerate() {
                let (Some(b), Some(p)) = (el.bounds(), el.picture_bounds(&ctx)) else {
                    println!("{} L{li}#{i}: no bounds", name.as_deref().unwrap_or("?"));
                    continue;
                };
                let (x0, y0) = ((p.0 + EPS).floor(), (p.1 + EPS).floor());
                let (x1, y1) = ((p.0 + p.2 - EPS).ceil(), (p.1 + p.3 - EPS).ceil());
                println!(
                    "{} L{li}#{i} {:?} {:.3},{:.3} {:.3}x{:.3} picture {}x{} room L{:.3} T{:.3} R{:.3} B{:.3} effects {}",
                    name.as_deref().unwrap_or("?"),
                    std::mem::discriminant(&el.kind),
                    b.0, b.1, b.2, b.3,
                    x1 - x0, y1 - y0,
                    b.0 - x0, b.1 - y0, x1 - b.0 - b.2, y1 - b.1 - b.3,
                    el.effects.len(),
                );
            }
        }
    }
    Ok(())
}
