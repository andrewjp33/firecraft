//! A document with every gradient type, an opacity ramp and every fill edge, made through the
//! same commands as the interface, to check against Fireworks MX: it writes `out.png` (a Fireworks
//! PNG to open in MX) and compares FireCraft's render with a flat PNG that MX re-saved.
//!
//!   cargo run -p firecraft-engine --example gradient_probe -- out.png
//!   cargo run -p firecraft-engine --example gradient_probe -- out.png fireworks_resave.png [diff.png]

use firecraft_doc::Document;
use firecraft_engine::{DocState, GRADIENT_FILLS, ToolOptions, opaque_ramp, preset_colors};
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let [_, out, rest @ ..] = a.as_slice() else {
        return Err("usage: gradient_probe OUT.png [FIREWORKS.png [DIFF.png]]".into());
    };
    let mut d = DocState::new(Document::new(800, 600, 72.0, Some(Rgba::WHITE)));
    let o = ToolOptions {
        stroke: None,
        fill: Some(Rgba::new(255, 0, 0, 255)),
        ..ToolOptions::default()
    };
    let spectrum = firecraft_engine::GRADIENT_PRESETS[10].1;
    let cell = |i: usize| {
        let (x, y) = (20.0 + 190.0 * (i % 4) as f64, 20.0 + 140.0 * (i / 4) as f64);
        (x, y, x + 170.0, y + 120.0)
    };
    for (i, (category, _)) in GRADIENT_FILLS.iter().enumerate() {
        d.add_shape(cell(i), i % 2 == 1, &o);
        d.set_fill_category(category, &o);
        d.edit_gradient("Edit Gradient", false, |g| {
            g.colors = preset_colors(spectrum);
            g.opacity = opaque_ramp();
        });
    }
    // An opacity ramp.
    d.add_shape(cell(11), false, &o);
    d.set_fill_category("fc_Linear", &o);
    d.edit_gradient("Edit Gradient", false, |g| {
        g.opacity = vec![(0.0, 255), (0.4, 40), (1.0, 200)]
    });
    // Fill edges: hard, feathered solid, feathered gradient.
    d.add_shape(cell(12), true, &o);
    d.set_fill_edge(false, 0, false);
    d.add_shape(cell(13), true, &o);
    d.set_fill_edge(true, 10, false);
    d.add_shape(cell(14), false, &o);
    d.set_fill_category("fc_Circular", &o);
    d.set_fill_edge(true, 22, false);
    std::fs::write(out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    println!("wrote {out}");
    if let Some(want) = rest.first() {
        // Compare what FireCraft renders from its saved file with Fireworks' render of it.
        let saved = firecraft_io::open(&std::fs::read(out)?, None)?;
        let got = saved.render(0);
        let want = firecraft_io::textures::decode_image(&std::fs::read(want)?)?;
        let (w, h) = (got.width().min(want.width()), got.height().min(want.height()));
        let mut diff = RgbaImage::new(w, h, Rgba::WHITE);
        let (mut differ, mut over2, mut worst) = (0u64, 0u64, 0i32);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (p, q) = (
                    got.get(x, y).unwrap_or_default(),
                    want.get(x, y).unwrap_or_default(),
                );
                let e = [
                    p.r as i32 - q.r as i32,
                    p.g as i32 - q.g as i32,
                    p.b as i32 - q.b as i32,
                    p.a as i32 - q.a as i32,
                ]
                .iter()
                .map(|v| v.abs())
                .max()
                .unwrap_or(0);
                if e > 0 {
                    differ += 1;
                    worst = worst.max(e);
                    if e > 2 {
                        over2 += 1;
                        diff.set(x, y, Rgba::new(255, 0, 0, 255));
                    }
                }
            }
        }
        println!(
            "{differ} of {} pixels differ; {over2} by more than 2 levels; worst {worst}",
            w as u64 * h as u64
        );
        if let Some(path) = rest.get(1) {
            std::fs::write(path, firecraft_io::encode_png(&diff, 72.0)?)?;
        }
    }
    Ok(())
}
