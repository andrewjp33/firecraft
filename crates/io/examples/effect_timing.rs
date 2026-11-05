//! Time each object's rasterising and each of its effects: effect_timing <file.png>
use firecraft_doc::{Element, RenderCtx};
use firecraft_raster::{Rgba, RgbaImage};
use std::time::Instant;

fn walk(el: &Element, ctx: &RenderCtx, depth: usize) {
    if let firecraft_doc::ElementKind::Group(children) = &el.kind {
        if el.effects.iter().all(|e| e.active().is_none()) {
            for c in children {
                walk(c, ctx, depth + 1);
            }
            return;
        }
    }
    let Some((x, y, w, h)) = el.full_region(ctx) else {
        return;
    };
    let t = Instant::now();
    let plain = {
        let mut e = el.clone();
        e.effects.clear();
        e.pixels(x, y, w, h, ctx)
    };
    let raster = t.elapsed();
    print!(
        "{:indent$}{:?} {}x{} raster {:.0} ms",
        "",
        std::mem::discriminant(&el.kind),
        w,
        h,
        raster.as_secs_f64() * 1000.0,
        indent = depth * 2
    );
    if let Some((img, _, _)) = plain {
        let m = el.effect_margin();
        let mut buf = RgbaImage::new(
            img.width() + 2 * m as u32,
            img.height() + 2 * m as u32,
            Rgba::TRANSPARENT,
        );
        buf.draw(&img, m, m, 255);
        for e in &el.effects {
            if let Some(fx) = e.active() {
                let t = Instant::now();
                fx.apply_with(&mut buf, ctx.math);
                print!(", {} {:.0} ms", e.name(), t.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    println!();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
        println!("== {path}");
        let ctx = RenderCtx {
            symbols: &doc.symbols,
            frame: 0,
            depth: 0,
            math: doc.color_math,
            chain: [0; firecraft_doc::MAX_SYMBOL_DEPTH as usize],
        };
        for l in &doc.layers {
            for el in l.elements(0) {
                walk(el, &ctx, 0);
            }
        }
    }
    Ok(())
}
