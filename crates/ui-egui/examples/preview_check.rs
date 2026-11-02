//! Compare a live transform preview with the real transform: preview_check <file.png> X Y
//! (the object under canvas point X,Y is scaled to 70% width about its left edge).
use firecraft_engine::DocState;
use firecraft_raster::transform::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("file")?)?, None)?;
    let (x, y): (f64, f64) = (a.get(2).ok_or("x")?.parse()?, a.get(3).ok_or("y")?.parse()?);
    let mut d = DocState::new(doc);
    let hit = d.hit_test(x, y).ok_or("nothing there")?;
    d.selection = vec![hit];
    let (bx, by, bw, bh) = d.selection_bounds().ok_or("no bounds")?;
    println!(
        "hit {hit:?} bounds {bx},{by} {bw}x{bh} kind {:?}",
        d.element(hit).map(|e| std::mem::discriminant(&e.kind))
    );
    let originals = d.selected_elements();
    let quad = [
        (bx, by),
        (bx + bw * 0.7, by),
        (bx + bw * 0.7, by + bh),
        (bx, by + bh),
    ];
    let m = Affine::rect_to_quad((bx, by, bw, bh), quad).ok_or("matrix")?;
    let baked = d.bake(&originals);
    for (r, e, img, x, y) in &baked {
        if let firecraft_doc::ElementKind::Bitmap(b) = &e.kind {
            let same = img.width() == b.image.width()
                && img.height() == b.image.height()
                && img.pixels() == b.image.pixels();
            println!(
                "{r:?}: baked {}x{} at {x},{y}; bitmap {}x{} at {},{}; identical pixels: {same}; opacity {} blend {:?} effects {}",
                img.width(),
                img.height(),
                b.image.width(),
                b.image.height(),
                b.x,
                b.y,
                e.opacity,
                e.blend,
                e.effects.len()
            );
        }
    }
    d.begin_live_edit("Transform");
    d.preview_transform(&baked, &m);
    let preview = d.doc.render(0);
    d.cancel_live_edit();
    d.transform_from(&originals, &m, "Scale");
    let real = d.doc.render(0);
    let (mut n, mut mx) = (0, 0u8);
    for (p, q) in preview.pixels().iter().zip(real.pixels()) {
        let dd =
            p.r.abs_diff(q.r)
                .max(p.g.abs_diff(q.g))
                .max(p.b.abs_diff(q.b))
                .max(p.a.abs_diff(q.a));
        mx = mx.max(dd);
        n += usize::from(dd > 8);
    }
    println!(
        "pixels differing by more than 8: {n} of {} (max {mx})",
        preview.pixels().len()
    );
    Ok(())
}
