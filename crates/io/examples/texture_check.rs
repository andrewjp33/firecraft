//! Write each texture of a folder as a PGM mask: texture_check <dir> <out_dir>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(a.get(2).ok_or("out")?);
    for (name, path) in firecraft_io::textures::files_in(std::path::Path::new(a.get(1).ok_or("dir")?)) {
        match firecraft_io::textures::texture_from_file(&path) {
            Some(t) => {
                let mut pgm = format!("P5\n{} {}\n255\n", t.width, t.height).into_bytes();
                pgm.extend_from_slice(&t.mask);
                std::fs::write(out.join(format!("{name}.pgm")), pgm)?;
            }
            None => println!("FAILED {name}"),
        }
    }
    Ok(())
}
