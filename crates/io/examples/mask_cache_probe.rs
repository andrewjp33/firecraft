//! How Fireworks treats a vector mask's cached render (`IMGv` before the content in a "mask to
//! image" group): mask_cache_probe <in.png> <outdir>. Lists the top-level masked objects, then
//! for the first one with a cache writes variants with the mask moved 40 px right:
//! `stale.png` (cache left where it was), `nocache.png` (cache dropped), `moved.png` (cache
//! moved with the mask). Open them in Fireworks MX to see which it draws.
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, outdir] = a.as_slice() else {
        return Err("usage: mask_cache_probe <in.png> <outdir>".into());
    };
    let doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    let mut target = None;
    for (li, l) in doc.layers.iter().enumerate() {
        for (ei, e) in l.elements(0).iter().enumerate() {
            if let ElementKind::Masked { mask, mode, cache, .. } = &e.kind {
                println!(
                    "layer {li} #{ei}: {mode:?} mask={} bounds={:?} cache={:?}",
                    mask.label(),
                    mask.bounds(),
                    cache.as_ref().and_then(|c| c.bounds())
                );
                if cache.is_some() && target.is_none() {
                    target = Some((li, ei));
                }
            }
        }
    }
    // How close FireCraft's render of each cache is to the one in the file.
    let ctx = firecraft_doc::RenderCtx { symbols: &doc.symbols, ..firecraft_doc::RenderCtx::EMPTY };
    for l in &doc.layers {
        for e in l.elements(0) {
            let ElementKind::Masked { mask, cache: Some(c), .. } = &e.kind else { continue };
            let (ElementKind::Bitmap(want), Some(got)) = (&c.kind, firecraft_doc::mask_cache(mask, &ctx)) else {
                continue;
            };
            let ElementKind::Bitmap(got) = &got.kind else { continue };
            let (mut diff, mut max, mut n) = (0u64, 0u8, 0u64);
            for y in 0..want.image.height() as i64 {
                for x in 0..want.image.width() as i64 {
                    let w = want.image.get(x, y).unwrap_or(firecraft_raster::Rgba::TRANSPARENT);
                    let g = got
                        .image
                        .get(x + want.x - got.x, y + want.y - got.y)
                        .unwrap_or(firecraft_raster::Rgba::TRANSPARENT);
                    if (w.r, w.g, w.b, w.a) != (g.r, g.g, g.b, g.a) && std::env::var("SHOW").is_ok() && n % 97 == 0 {
                        println!("  at {},{}: file {:?} FireCraft {:?}", x + want.x, y + want.y, w, g);
                    }
                    // Colour under alpha 0 doesn't show (Fireworks leaves it 0 or 255).
                    let pairs = if w.a == 0 && g.a == 0 {
                        [(0, 0); 4]
                    } else {
                        [(w.r, g.r), (w.g, g.g), (w.b, g.b), (w.a, g.a)]
                    };
                    for (a, b) in pairs {
                        let d = a.abs_diff(b);
                        diff += d as u64;
                        max = max.max(d);
                    }
                    n += 1;
                }
            }
            println!(
                "cache {}x{} at {},{} vs FireCraft {}x{} at {},{}: mean diff {:.3}, max {max}; file centre {:?}, FireCraft {:?}",
                want.image.width(), want.image.height(), want.x, want.y,
                got.image.width(), got.image.height(), got.x, got.y,
                diff as f64 / (n.max(1) * 4) as f64,
                want.image.get(want.image.width() as i64 / 2, want.image.height() as i64 / 2),
                got.image.get(want.x - got.x + want.image.width() as i64 / 2, want.y - got.y + want.image.height() as i64 / 2),
            );
        }
    }
    let Some((li, ei)) = target else {
        return Err("no masked object with a cache".into());
    };
    for variant in ["stale", "nocache", "moved"] {
        let mut d = doc.clone();
        let el = d.layers[li].elements_mut(0).and_then(|c| c.get_mut(ei)).ok_or("gone")?;
        if let ElementKind::Masked { mask, cache, .. } = &mut el.kind {
            mask.translate(40.0, 0.0);
            match variant {
                "nocache" => *cache = None,
                "moved" => {
                    if let Some(c) = cache {
                        c.translate(40.0, 0.0);
                    }
                }
                _ => {}
            }
        }
        let out = format!("{outdir}/{variant}.png");
        std::fs::write(&out, firecraft_io::save_fireworks_png(&d)?)?;
        println!("wrote {out}");
    }
    Ok(())
}
