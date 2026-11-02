//! Re-encode clipboard PNG data the way FireCraft writes it: clip_rewrite <in> <out>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let (els, syms, tiles) = firecraft_io::read_clipboard_png(&std::fs::read(a.get(1).ok_or("in")?)?)?;
    let b = els.iter().filter_map(|e| e.bounds()).reduce(|a, b| {
        let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
        (
            x0,
            y0,
            (a.0 + a.2).max(b.0 + b.2) - x0,
            (a.1 + a.3).max(b.1 + b.3) - y0,
        )
    });
    let png = firecraft_io::clipboard_png(&els, &syms, &tiles, b.ok_or("no bounds")?)?;
    std::fs::write(a.get(2).ok_or("out")?, png)?;
    Ok(())
}
