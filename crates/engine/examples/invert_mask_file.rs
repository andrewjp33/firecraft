//! Invert the mask of each listed top-level masked object (first layer) and save:
//! invert_mask_file <in.png> <out.png> <index>... (for checking the result in Fireworks).
use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (input, out) = (a.first().ok_or("in")?, a.get(1).ok_or("out")?);
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(input)?, None)?);
    for i in &a[2..] {
        d.invert_mask(ElementRef::new(0, i.parse()?));
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
