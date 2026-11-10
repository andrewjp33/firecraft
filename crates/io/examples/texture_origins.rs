//! Every textured path drawn through symbol instances in frame 1, with its texture origin and
//! bounds mapped to the canvas: texture_origins FILE.png
use firecraft_doc::{Document, Element, ElementKind};
use firecraft_raster::transform::Affine;

fn visit(doc: &Document, e: &Element, chain: &mut Vec<Affine>) {
    let map =
        |chain: &[Affine], (x, y): (f64, f64)| chain.iter().rev().fold((x, y), |p, m| m.apply(p.0, p.1));
    match &e.kind {
        ElementKind::Group(c) => c.iter().for_each(|x| visit(doc, x, chain)),
        ElementKind::Masked { content, .. } => content.iter().for_each(|x| visit(doc, x, chain)),
        ElementKind::Instance { matrix, symbol, .. } => {
            let Some(s) = doc.symbols.iter().find(|s| s.id == *symbol) else {
                return;
            };
            if !matrix.is_affine() {
                let (x, y) = matrix.apply(0.0, 0.0);
                println!("perspective instance: (0,0) -> {x:.3},{y:.3}; matrix {matrix:?}");
            }
            chain.push(*matrix);
            for c in s.layers.iter().flat_map(|l| l.elements(0)) {
                visit(doc, c, chain);
            }
            chain.pop();
        }
        ElementKind::Path(p) => {
            let o = firecraft_doc::stroke_fw::texture_origin(p);
            let (ox, oy) = map(chain, o);
            let (ix, iy) = map(&chain[1..], o);
            print!("inner {ix:.2},{iy:.2}  ");
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for q in p.contours.iter().flat_map(|c| c.points.iter()) {
                let (x, y) = map(chain, (q.x, q.y));
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            println!(
                "origin {:.2},{:.2} stored {} -> {ox:.3},{oy:.3}  bounds {x0:.3},{y0:.3}..{x1:.3},{y1:.3}  texture {} {}",
                o.0,
                o.1,
                p.texture_origin.is_some(),
                p.fill.as_ref().map_or(0, |f| f.texture_amount),
                p.fill
                    .as_ref()
                    .and_then(|f| f.texture.as_ref())
                    .map_or(String::new(), |t| format!("{}x{}", t.width, t.height)),
            );
        }
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    for e in doc.layers.iter().flat_map(|l| l.elements(0)) {
        visit(&doc, e, &mut Vec::new());
    }
    Ok(())
}
