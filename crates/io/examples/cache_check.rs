//! Check the render cache against uncached rendering, and time moving an object:
//! cache_check <file.png>...
use firecraft_doc::RenderCache;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let mut doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let t = Instant::now();
        let plain = doc.render(0);
        let t_plain = t.elapsed();
        let mut cache = RenderCache::new();
        let t = Instant::now();
        let cached = doc.render_cached(0, &mut cache);
        let t_first = t.elapsed();
        let same = plain == cached;
        if !same {
            let (mut n, mut mx, mut bb) = (0usize, 0u8, (u32::MAX, u32::MAX, 0u32, 0u32));
            for y in 0..plain.height() {
                for x in 0..plain.width() {
                    let (a, b) = (
                        plain.get(x as i64, y as i64).unwrap_or_default(),
                        cached.get(x as i64, y as i64).unwrap_or_default(),
                    );
                    let d =
                        a.r.abs_diff(b.r)
                            .max(a.g.abs_diff(b.g))
                            .max(a.b.abs_diff(b.b))
                            .max(a.a.abs_diff(b.a));
                    if d > 0 {
                        n += 1;
                        mx = mx.max(d);
                        bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
                    }
                }
            }
            println!(
                "    {n} pixels differ (max {mx}) within x {}..{} y {}..{} of {}x{}",
                bb.0,
                bb.2,
                bb.1,
                bb.3,
                plain.width(),
                plain.height()
            );
        }
        // Move each layer's top object by (7, 5) and re-render with the cache.
        for l in doc.layers.iter_mut() {
            if let Some(v) = l.elements_mut(0) {
                if let Some(e) = v.last_mut() {
                    e.translate(7.0, 5.0);
                }
            }
        }
        let t = Instant::now();
        let moved = doc.render_cached(0, &mut cache);
        let t_moved = t.elapsed();
        let hits = cache.last;
        let dirty = cache.dirty;
        let same_moved = moved == doc.render(0);
        println!(
            "{name:<20} uncached {:>6.0} ms  cached first {:>6.0} ms  after move {:>5.0} ms (hits {}, rendered {}, {dirty:?})  equal: {same}/{same_moved}",
            t_plain.as_secs_f64() * 1000.0,
            t_first.as_secs_f64() * 1000.0,
            t_moved.as_secs_f64() * 1000.0,
            hits.0,
            hits.1
        );
    }
    Ok(())
}
