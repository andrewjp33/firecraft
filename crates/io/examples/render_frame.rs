//! Render one frame of a document to a flat PNG: render_frame <in.png> <frame (1-based)> <out.png>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, frame, out] = a.as_slice() else {
        return Err("usage: render_frame <in.png> <frame> <out.png>".into());
    };
    let doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    let f: usize = frame.parse::<usize>()?.saturating_sub(1);
    println!(
        "{} frames; delays (1/100 s): {:?}",
        doc.frames.len(),
        doc.frames.iter().map(|f| f.delay).collect::<Vec<_>>()
    );
    std::fs::write(out, firecraft_io::encode_png(&doc.render(f), doc.resolution)?)?;
    Ok(())
}
