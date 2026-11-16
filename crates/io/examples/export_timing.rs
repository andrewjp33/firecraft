//! Time an export as the Optimize preview does it: `export_timing <file> [frame]`.
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: export_timing <file> [frame]")?;
    let mut doc = firecraft_io::open(&std::fs::read(&path)?, Some(path.clone()))?;
    doc.current_frame = args.next().map_or(Ok(0), |s| s.parse())?;
    println!("format {:?}, {} frames", doc.export.format, doc.frames.len());
    let t = Instant::now();
    for f in 0..doc.frames.len() {
        let _ = doc.render(f);
    }
    println!("rendering every frame {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    for _ in 0..2 {
        let t = Instant::now();
        let e = firecraft_io::export::export(&doc, &doc.export)?;
        println!("export {:.0} ms, {} bytes", t.elapsed().as_secs_f64() * 1000.0, e.bytes.len());
    }
    Ok(())
}
