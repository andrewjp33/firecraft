//! Compare FireCraft's Text > Convert to Paths with Fireworks MX's: `convert_compare SRC.png DST.png`,
//! where DST is SRC after MX converted all its text. [OUT.png]: FireCraft's conversion, saved.
use firecraft_doc::{Element, ElementKind};
use firecraft_engine::{DocState, ElementRef};

fn paths(e: &Element) -> Vec<&firecraft_doc::Path> {
    match &e.kind {
        ElementKind::Group(c) => c.iter().flat_map(paths).collect(),
        ElementKind::Path(p) => vec![p],
        _ => vec![],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(&a[1])?, None)?);
    let n = d.doc.layers[0].elements(0).len();
    d.selection = (0..n).map(|i| ElementRef::new(0, i)).collect();
    d.convert_text_to_paths();
    if let Some(out) = a.get(3) {
        std::fs::write(out, firecraft_io::save_fireworks_png(&d.doc)?)?;
    }
    let mx = firecraft_io::open(&std::fs::read(&a[2])?, None)?;
    let fc_els = d.doc.layers[0].elements(0);
    let mx_els = mx.layers[0].elements(0);
    println!("objects: fc {} mx {}", fc_els.len(), mx_els.len());
    for (k, (f, m)) in fc_els.iter().zip(mx_els).enumerate() {
        let (pf, pm) = (paths(f), paths(m));
        println!("object {k}: paths fc {} mx {}", pf.len(), pm.len());
        for (i, (p, q)) in pf.iter().zip(&pm).enumerate() {
            let shape =
                |p: &firecraft_doc::Path| p.contours.iter().map(|c| c.points.len()).collect::<Vec<_>>();
            let color = |p: &firecraft_doc::Path| p.fill.as_ref().map(|f| f.color);
            let mut worst = 0.0f64;
            if shape(p) == shape(q) {
                for (c, e) in p.contours.iter().zip(&q.contours) {
                    for (u, v) in c.points.iter().zip(&e.points) {
                        for d in [
                            u.x - v.x,
                            u.y - v.y,
                            u.pred.0 - v.pred.0,
                            u.pred.1 - v.pred.1,
                            u.succ.0 - v.succ.0,
                            u.succ.1 - v.succ.1,
                        ] {
                            worst = worst.max(d.abs());
                        }
                    }
                }
            }
            let first = |p: &firecraft_doc::Path| {
                p.contours
                    .first()
                    .and_then(|c| c.points.first())
                    .map(|q| (q.x, q.y))
            };
            println!(
                "  {i}: nodes fc {:?} mx {:?} worst {:.4} colour {} first fc {:?} mx {:?}",
                shape(p),
                shape(q),
                worst,
                if color(p) == color(q) {
                    "same".to_string()
                } else {
                    format!("{:?} vs {:?}", color(p), color(q))
                },
                first(p),
                first(q)
            );
        }
        for (who, list) in [("fc", &pf), ("mx", &pm)] {
            for p in list
                .iter()
                .filter(|p| p.contours.len() == 1 && p.contours[0].points.len() == 4)
            {
                let pts: Vec<(f64, f64)> = p.contours[0].points.iter().map(|q| (q.x, q.y)).collect();
                println!("  {who} rectangle {pts:?} {:?}", p.fill.as_ref().map(|f| f.color));
            }
        }
        println!("  effects fc {} mx {}", f.effects.len(), m.effects.len());
    }
    Ok(())
}
