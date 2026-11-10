//! Attach the text in a two-object file to its path and save, for checking in Fireworks MX:
//! `attach_probe IN.png OUT.png`
use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(&a[1])?, None)?);
    let n = d.doc.layers[0].elements(0).len();
    d.selection = (0..n).map(|i| ElementRef::new(0, i)).collect();
    d.attach_text_to_path();
    std::fs::write(&a[2], firecraft_io::save_fireworks_png(&d.doc)?)?;
    println!("elements now {}", d.doc.layers[0].elements(0).len());
    Ok(())
}
