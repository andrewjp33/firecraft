//! For a document whose top-level object `index` is masked by a bitmap: print, at sample points,
//! the mask pixel (as FireCraft holds it, and as stored in the file), the content and what lies
//! below, and Fireworks' stored composite against FireCraft's render, so the mask strength
//! Fireworks used can be worked out: mask_probe_pixels <file.png> <index> <x,y>...
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (path, index) = (a.first().ok_or("file")?, a.get(1).ok_or("index")?.parse::<usize>()?);
    let bytes = std::fs::read(path)?;
    let doc = firecraft_io::open(&bytes, None)?;
    let flat = firecraft_io::decode_png(&bytes)?;
    let mine = doc.render(0);
    let els = doc.layers[0].elements(0);
    let ElementKind::Masked { mask, content, .. } = &els[index].kind else { return Err("not masked".into()) };
    let ElementKind::Bitmap(m) = &mask.kind else { return Err("not a bitmap mask".into()) };
    let ElementKind::Bitmap(c) = &content[0].kind else { return Err("content not a bitmap".into()) };
    for p in &a[2..] {
        let (x, y) = p.split_once(',').ok_or("x,y")?;
        let (x, y): (i64, i64) = (x.parse()?, y.parse()?);
        let mp = m.image.get(x - m.x, y - m.y);
        let cp = c.image.get(x - c.x, y - c.y);
        println!(
            "{x},{y}: mask {:?} content {:?} fw {:?} fc {:?}",
            mp.map(|p| (p.r, p.g, p.b, p.a)),
            cp.map(|p| (p.r, p.a)),
            flat.get(x, y).map(|p| (p.r, p.g, p.b, p.a)),
            mine.get(x, y).map(|p| (p.r, p.g, p.b, p.a))
        );
    }
    Ok(())
}
