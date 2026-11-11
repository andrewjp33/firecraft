//! Print the export settings a file opens with: export_info <file>...
//! With EXPORT_DIR set, also export each file with those settings into that folder (PNG_LEVEL
//! overrides the PNG optimization level).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("EXPORT_DIR").ok();
    for path in std::env::args().skip(1) {
        let doc = firecraft_io::open(&std::fs::read(&path)?, Some(path.clone()))?;
        let mut s = doc.export.clone();
        s.raw = None;
        println!("{path}: {s:?}");
        if let Some(dir) = &out_dir {
            let mut s = doc.export.clone();
            if let Some(l) = std::env::var("PNG_LEVEL").ok().and_then(|v| v.parse().ok()) {
                s.png_optimize = l;
            }
            let t = std::time::Instant::now();
            let bytes = firecraft_io::export::export_file(&doc, &s)?;
            let name = firecraft_io::export::file_name(&doc.title(), s.format);
            std::fs::write(format!("{dir}/{name}"), &bytes)?;
            println!("  -> {dir}/{name}: {} bytes in {:.2} s", bytes.len(), t.elapsed().as_secs_f64());
        }
    }
    Ok(())
}
