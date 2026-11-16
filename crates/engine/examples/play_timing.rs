//! Time playing a document's frames as the canvas does (render_cached, one cache): two passes,
//! `play_timing <file.png> [frames]`. `play_timing --check <file.png>...` instead compares every
//! frame's cached render, one cache going through them in order, with an uncached render.
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "--check") {
        args.next();
        for path in args {
            let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
            let mut cache = firecraft_doc::RenderCache::new();
            let n = doc.frames.len().max(1);
            let mut bad = Vec::new();
            for pass in 0..2 {
                for f in 0..n {
                    let (a, b) = (doc.render_cached(f, &mut cache), doc.render(f));
                    if a != b {
                        let (mut n, mut worst) = (0usize, 0u8);
                        for (p, q) in a.pixels().iter().zip(b.pixels()) {
                            let d = [p.r.abs_diff(q.r), p.g.abs_diff(q.g), p.b.abs_diff(q.b), p.a.abs_diff(q.a)];
                            let m = d.into_iter().max().unwrap_or(0);
                            if m > 0 {
                                n += 1;
                                worst = worst.max(m);
                            }
                        }
                        bad.push((pass, f, n, worst));
                    }
                }
            }
            println!("{path}: {n} frames, {}", if bad.is_empty() { "identical".to_owned() } else { format!("DIFFER {bad:?}") });
        }
        return Ok(());
    }
    let path = args.next().ok_or("usage: play_timing <file.png> [frames]")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    let n = args.next().map_or(Ok(doc.frames.len().max(1)), |s| s.parse())?;
    let mut cache = firecraft_doc::RenderCache::new();
    for pass in 0..2 {
        let t = Instant::now();
        let mut worst = (0.0, 0);
        for f in 0..n {
            let s = Instant::now();
            doc.render_cached(f, &mut cache);
            let ms = s.elapsed().as_secs_f64() * 1000.0;
            if ms > worst.0 {
                worst = (ms, f);
            }
            if pass == 0 && f < 4 {
                println!("  frame {f}: {ms:.1} ms {:?}", cache.last);
            }
        }
        println!(
            "pass {pass}: {n} frames {:.0} ms ({:.1} ms/frame), worst frame {} {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0,
            t.elapsed().as_secs_f64() * 1000.0 / n as f64,
            worst.1,
            worst.0
        );
    }
    Ok(())
}
