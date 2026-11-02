//! Show the objects in clipboard PNG data (Fireworks' "PNG" clipboard format): clip_read <file>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: clip_read <file>")?;
    let (els, syms) = firecraft_io::read_clipboard_png(&std::fs::read(path)?)?;
    for e in &els {
        println!("{} {:?}", e.label(), e.bounds());
    }
    println!("{} objects, {} symbols", els.len(), syms.len());
    Ok(())
}
