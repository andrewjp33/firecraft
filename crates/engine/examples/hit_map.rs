//! Compare the Pointer's hit test with the rendered pixels around a point:
//! hit_map <file.png> <x> <y> <radius> <step>   ('#' hit and visible, 'v' visible only, 'h' hit only)
use firecraft_engine::DocState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("file")?)?, None)?;
    let (cx, cy, r, step): (i64, i64, i64, i64) =
        (a[2].parse()?, a[3].parse()?, a[4].parse()?, a[5].parse()?);
    let img = doc.render(0);
    let d = DocState::new(doc);
    for y in (cy - r..=cy + r).step_by(step as usize) {
        let row: String = (cx - r..=cx + r)
            .step_by(step as usize)
            .map(|x| {
                let vis = img.get(x, y).is_some_and(|p| p.r > 40 || p.g > 40);
                let hit = !d.hits_at(x as f64 + 0.5, y as f64 + 0.5).is_empty();
                match (hit, vis) {
                    (true, true) => '#',
                    (false, true) => 'v',
                    (true, false) => 'h',
                    _ => '.',
                }
            })
            .collect();
        println!("{y:5} {row}");
    }
    Ok(())
}
