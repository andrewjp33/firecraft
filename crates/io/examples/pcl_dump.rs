//! First path's stored polyline (`PCLv`) next to FireCraft's flattening: pcl_dump FILE.png
use firecraft_doc::{Element, ElementKind};

fn first_path(e: &Element) -> Option<&firecraft_doc::Path> {
    match &e.kind {
        ElementKind::Path(p) => Some(p),
        ElementKind::Group(c) => c.iter().find_map(first_path),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage")?;
    let doc = firecraft_io::open(&std::fs::read(&path)?, None)?;
    let p = doc.layers[0]
        .elements(0)
        .iter()
        .find_map(first_path)
        .ok_or("no path")?;
    if let Some(pcl) = &p.pcl {
        let l = &pcl.lines[0];
        println!(
            "PCL {} points, closed {}: first {:?}",
            l.points.len(),
            l.closed,
            l.points
                .iter()
                .take(6)
                .map(|q| (q.x, q.y, q.pressure, q.velocity))
                .collect::<Vec<_>>()
        );
        println!("     source matches contours: {}", pcl.source == p.contours);
    } else {
        println!("no PCL");
    }
    let c = &p.contours[0];
    println!(
        "contour {} points: first {:?}",
        c.points.len(),
        c.points.iter().take(3).map(|q| (q.x, q.y)).collect::<Vec<_>>()
    );
    let flat = c.flatten(0.0, 0.0);
    println!(
        "FireCraft flatten {} points: first {:?}",
        flat.len(),
        flat.iter().take(6).collect::<Vec<_>>()
    );
    Ok(())
}
