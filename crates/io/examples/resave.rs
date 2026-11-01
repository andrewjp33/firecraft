//! Open and re-save a document as a Fireworks PNG: resave <in.png> <out.png> [--precise|--classic]
//! (optionally switching its colour math). Handy for checking files in Fireworks MX.
use firecraft_raster::ColorMath;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (input, out) = match a.as_slice() {
        [i, o, ..] => (i, o),
        _ => return Err("usage: resave <in.png> <out.png> [--precise|--classic]".into()),
    };
    let mut doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    match a.get(2).map(String::as_str) {
        Some("--precise") => doc.color_math = ColorMath::Precise,
        Some("--classic") => doc.color_math = ColorMath::Classic,
        _ => {}
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&doc)?)?;
    let back = firecraft_io::open(&std::fs::read(out)?, None)?;
    println!("saved {out}: colour math {:?}", back.color_math);
    Ok(())
}
