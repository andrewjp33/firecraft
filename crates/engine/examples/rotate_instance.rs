//! Rotate the first symbol instance of a document about its centre and render it:
//! rotate_instance <in.png> <degrees> <out.png>
use firecraft_doc::ElementKind;
use firecraft_raster::transform::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("in")?)?, None)?;
    let deg: f64 = a.get(2).ok_or("degrees")?.parse()?;
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
    if let Some(ElementKind::Instance { matrix, .. }) =
        doc.layers.get_mut(li).and_then(|l| l.cells.get_mut(ci)).and_then(|c| c.get_mut(ei)).map(|e| &mut e.kind)
    {
        let (cx, cy) = (matrix.e, matrix.f);
        let rot = Affine { a: c, b: s, c: -s, d: c, ..Affine::IDENTITY };
        *matrix = Affine::translate(cx, cy).then(&rot).then(&Affine::translate(-cx, -cy)).then(matrix);
    }
    let img = doc.render(0);
    std::fs::write(a.get(3).ok_or("out")?, firecraft_io::encode_png(&img, 72.0)?)?;
    Ok(())
}
