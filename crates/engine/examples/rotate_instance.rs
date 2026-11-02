//! Rotate the most scaled-up symbol instance of a document about its centre (or squeeze its
//! top corners in, a perspective transform, with `pinch`) and render it:
//! rotate_instance <in.png> <degrees|pinch> <out.png>
use firecraft_doc::ElementKind;
use firecraft_raster::transform::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("in")?)?, None)?;
    let mode = a.get(2).ok_or("degrees or pinch")?.clone();
    let deg: f64 = if mode == "pinch" { 0.0 } else { mode.parse()? };
    let (s, c) = deg.to_radians().sin_cos();
    // The most scaled-up instance.
    let mut best: Option<(usize, usize, usize, f64)> = None;
    for (li, l) in doc.layers.iter().enumerate() {
        for (ci, cell) in l.cells.iter().enumerate() {
            for (ei, e) in cell.iter().enumerate() {
                if let ElementKind::Instance { matrix, .. } = &e.kind
                    && best.is_none_or(|b| matrix.a > b.3)
                {
                    best = Some((li, ci, ei, matrix.a));
                }
            }
        }
    }
    let (li, ci, ei, _) = best.ok_or("no instance")?;
    if let Some(ElementKind::Instance { matrix, .. }) = doc
        .layers
        .get_mut(li)
        .and_then(|l| l.cells.get_mut(ci))
        .and_then(|c| c.get_mut(ei))
        .map(|e| &mut e.kind)
    {
        let (cx, cy) = (matrix.e, matrix.f);
        if mode == "pinch" {
            // The instance's current box, top corners pulled in by 15% of its width.
            let r = (-100.0, -100.0, 200.0, 200.0);
            let q = [
                matrix.apply(-100.0, -100.0),
                matrix.apply(100.0, -100.0),
                matrix.apply(100.0, 100.0),
                matrix.apply(-100.0, 100.0),
            ];
            let inset = (q[1].0 - q[0].0) * 0.15;
            let pinched = [(q[0].0 + inset, q[0].1), (q[1].0 - inset, q[1].1), q[2], q[3]];
            if let Some(m) = Affine::rect_to_quad(r, pinched) {
                *matrix = m;
            }
        } else {
            let rot = Affine {
                a: c,
                b: s,
                c: -s,
                d: c,
                ..Affine::IDENTITY
            };
            *matrix = Affine::translate(cx, cy)
                .then(&rot)
                .then(&Affine::translate(-cx, -cy))
                .then(matrix);
        }
    }
    let img = doc.render(0);
    std::fs::write(a.get(3).ok_or("out")?, firecraft_io::encode_png(&img, 72.0)?)?;
    Ok(())
}
