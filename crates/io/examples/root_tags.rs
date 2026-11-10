//! A Fireworks PNG's root record, top level only (values of short leaves): root_tags FILE.png
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let fw = firecraft_fwpng::FwPng::read(&std::fs::read(&path)?)?;
    let root = fw.tree()?.ok_or("no tree")?;
    for c in &root.children {
        let tag = String::from_utf8_lossy(&c.tag).into_owned();
        if c.children.is_empty() {
            println!("{tag} {}", c.text.chars().take(40).collect::<String>());
        } else {
            println!("{tag} ({} children)", c.children.len());
        }
    }
    Ok(())
}
