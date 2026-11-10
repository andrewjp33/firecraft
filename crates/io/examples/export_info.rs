//! Print the export settings a file opens with: export_info <file>...
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let doc = firecraft_io::open(&std::fs::read(&path)?, Some(path.clone()))?;
        let mut s = doc.export.clone();
        s.raw = None;
        println!("{path}: {s:?}");
    }
    Ok(())
}
