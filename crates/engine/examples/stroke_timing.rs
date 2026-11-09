//! Time a large path stroke: `stroke_timing [SIZE]` in the build profile it was compiled with.
//! `stroke_timing --hash` prints a checksum of every preset's render, to compare before and after
//! a change that mustn't alter any pixel.

use std::time::Instant;

use firecraft_doc::Document;
use firecraft_engine::{DocState, ToolOptions};
use firecraft_raster::Rgba;

fn checksum() {
    let shapes = [
        ((200.0, 150.0, 600.0, 450.0), false),
        ((120.0, 90.0, 690.0, 510.0), true),
    ];
    for (i, preset) in firecraft_doc::brush_presets::BRUSH_PRESETS.iter().enumerate() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for size in [1u32, 7, 40] {
            for (rect, ellipse) in shapes {
                for placement in 0..3u8 {
                    let mut d = DocState::new(Document::new(800, 600, 72.0, None));
                    let o = ToolOptions {
                        stroke: Some(Rgba::new(20, 90, 200, 255)),
                        fill: None,
                        stroke_size: size,
                        stroke_preset: i,
                        stroke_placement: placement,
                        ..ToolOptions::default()
                    };
                    d.add_shape(rect, ellipse, &o);
                    for p in d.doc.render(0).pixels() {
                        for b in [p.r, p.g, p.b, p.a] {
                            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
                        }
                    }
                }
            }
        }
        println!("{:40} {h:016x}", preset.name);
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--hash") {
        checksum();
        return;
    }
    let size: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    for (name, preset) in firecraft_doc::brush_presets::BRUSH_PRESETS
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            [
                "bn_Soft Line",
                "bn_1-Pixel Anti-Aliased",
                "bn_Soft Rounded",
                "bn_Basic",
                "bn_Hard Line",
                "bn_Hard Rounded",
            ]
            .contains(&p.name)
        })
        .map(|(i, p)| (p.name, i))
    {
        let mut d = DocState::new(Document::new(800, 600, 72.0, None));
        let o = ToolOptions {
            stroke: Some(Rgba::BLACK),
            fill: None,
            stroke_size: size,
            stroke_preset: preset,
            ..ToolOptions::default()
        };
        d.add_shape((200.0, 150.0, 600.0, 450.0), false, &o);
        let t = Instant::now();
        let first = d.doc.render(0);
        let a = t.elapsed();
        d.doc.layers[0].elements_mut(0).expect("layer")[0].translate(1.0, 0.0);
        let t = Instant::now();
        let second = d.doc.render(0);
        let b = t.elapsed();
        let mut cache = firecraft_doc::RenderCache::default();
        let t = Instant::now();
        d.doc.render_cached(0, &mut cache);
        let c0 = t.elapsed();
        d.doc.layers[0].elements_mut(0).expect("layer")[0].translate(3.0, 2.0);
        let t = Instant::now();
        d.doc.render_cached(0, &mut cache);
        let c1 = t.elapsed();
        println!(
            "{name:28} cached: first {:6.1} ms   whole-pixel move {:6.1} ms",
            c0.as_secs_f64() * 1e3,
            c1.as_secs_f64() * 1e3
        );
        println!(
            "{name:28} first {:7.1} ms   moved {:7.1} ms  ({}x{})",
            a.as_secs_f64() * 1e3,
            b.as_secs_f64() * 1e3,
            first.width(),
            second.height()
        );
    }
}
