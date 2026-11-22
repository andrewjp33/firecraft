//! A benchmark of FireCraft's pixel work, for comparing the two browser builds (modern
//! WebAssembly, and the plain build for older browsers) in the same browser. Load the page with
//! `?wasm=modern` or `?wasm=legacy` to pick the build, then in the browser's console:
//!
//! ```js
//! wasmBindings.firecraft_benchmark()                 // synthetic tests only
//! wasmBindings.firecraft_benchmark(new Uint8Array(await (await fetch("doc.png")).arrayBuffer()))
//! ```
//!
//! It returns one line per test: the name and the median of five runs, in milliseconds.

use std::hint::black_box;

use firecraft_raster::effects::Effect;
use firecraft_raster::transform::{Resample, resize_image};
use firecraft_raster::{Rgba, RgbaImage};
use wasm_bindgen::prelude::*;

const RUNS: usize = 5;

fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
}

/// Median time of `RUNS` runs after one warm-up run, in milliseconds.
fn time(mut f: impl FnMut()) -> f64 {
    f();
    let mut times: Vec<f64> = (0..RUNS)
        .map(|_| {
            let start = now();
            f();
            now() - start
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times.get(RUNS / 2).copied().unwrap_or(f64::NAN)
}

/// Varied colours with a soft-edged round alpha, like an object with effects on it.
fn test_image(w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h, Rgba::new(0, 0, 0, 0));
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let r = w.min(h) as f32 * 0.45;
    for (i, p) in img.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let a = ((r - d) * 0.5).clamp(0.0, 1.0);
        *p = Rgba::new(
            (x * 255.0 / w as f32) as u8,
            (y * 255.0 / h as f32) as u8,
            ((x * 3.0 + y * 5.0) as u32 % 256) as u8,
            (a * 255.0) as u8,
        );
    }
    img
}

#[wasm_bindgen]
pub fn firecraft_benchmark(document: Option<Vec<u8>>) -> String {
    let mut out = String::new();
    let mut line = |name: &str, ms: f64| out.push_str(&format!("{name}\t{ms:.1}\n"));

    let src = vec![7u8; 64 << 20];
    let mut dst = vec![0u8; 64 << 20];
    line("copy 64 MB", time(|| dst.copy_from_slice(black_box(&src))));
    line("fill 64 MB", time(|| black_box(&mut dst).fill(black_box(1))));

    let base = test_image(1600, 1200);
    line("make test image 1600x1200", time(|| {
        black_box(test_image(1600, 1200));
    }));
    let shadow = Rgba::new(0, 0, 0, 166);
    let effects = [
        ("Gaussian Blur 8", Effect::GaussianBlur { radius: 8.0 }),
        ("Motion Blur 20", Effect::MotionBlur { distance: 20.0, angle: 30.0 }),
        ("Drop Shadow", Effect::Shadow {
            color: shadow,
            distance: 7.0,
            angle: 315.0,
            softness: 4.0,
            inner: false,
            knock_out: false,
        }),
        ("Glow", Effect::Glow {
            color: Rgba::new(255, 200, 0, 255),
            width: 8.0,
            opacity: 0.75,
            softness: 4.0,
            offset: 0.0,
            inner: false,
        }),
        ("Hue/Saturation", Effect::HueSaturation {
            hue: 30.0,
            saturation: 20.0,
            lightness: 10.0,
            colorize: false,
        }),
        ("Unsharp Mask", Effect::UnsharpMask { radius: 4.1, amount: 50.0, threshold: 0 }),
    ];
    for (name, effect) in &effects {
        line(name, time(|| {
            let mut img = base.clone();
            effect.apply(&mut img);
            black_box(img);
        }));
    }
    line("Resize bicubic to 2400x1800", time(|| {
        black_box(resize_image(&base, 2400, 1800, Resample::Bicubic));
    }));

    if let Some(bytes) = document {
        match firecraft_io::open(&bytes, Some("benchmark.png".into())) {
            Ok(doc) => {
                line("open document", time(|| {
                    black_box(firecraft_io::open(&bytes, None).ok());
                }));
                line("render document", time(|| {
                    black_box(doc.render(0));
                }));
            }
            Err(e) => out.push_str(&format!("document didn't open: {e}\n")),
        }
    }
    out
}
