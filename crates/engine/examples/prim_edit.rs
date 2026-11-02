//! Edit the three rectangles of a document (by their left edge: the leftmost is moved, the
//! rightmost rotated and scaled, the middle one has a corner dragged) and save:
//! prim_edit <in.png> <out.png>
use firecraft_doc::ElementKind;
use firecraft_raster::transform::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("in")?)?, None)?;
    let cell = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.cells.get_mut(0))
        .find(|c| c.len() == 3)
        .ok_or("no layer with three objects")?;
    let left = |e: &firecraft_doc::Element| match &e.kind {
        ElementKind::Path(p) => p
            .contours
            .iter()
            .flat_map(|c| c.points.iter())
            .map(|p| p.x)
            .fold(f64::MAX, f64::min),
        _ => f64::MAX,
    };
    let mut order: Vec<usize> = (0..cell.len()).collect();
    order.sort_by(|&i, &j| left(&cell[i]).total_cmp(&left(&cell[j])));
    let [l, m, r] = order[..] else {
        return Err("need three objects".into());
    };
    cell[l].translate(316.0, 119.0);
    let (s, c) = 20f64.to_radians().sin_cos();
    let rot = Affine {
        a: 1.2 * c,
        b: 1.2 * s,
        c: -0.8 * s,
        d: 0.8 * c,
        ..Affine::IDENTITY
    };
    // About the rectangle's corner (385,128): rotate/scale, then put the corner back.
    let (px, py) = (385.0, 128.0);
    let (qx, qy) = rot.apply(px, py);
    cell[r].transform(&Affine {
        e: px - qx,
        f: py - qy,
        ..rot
    });
    if let ElementKind::Path(p) = &mut cell[m].kind
        && let Some(pt) = p.contours.get_mut(0).and_then(|c| c.points.get_mut(2))
    {
        pt.x += 40.0;
        pt.y += 25.0;
    }
    std::fs::write(a.get(2).ok_or("out")?, firecraft_io::save_fireworks_png(&doc)?)?;
    Ok(())
}
