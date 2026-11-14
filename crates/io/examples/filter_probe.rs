//! Probe document for Fireworks MX's classic filters as Live Effects (Blur, Blur More, Sharpen,
//! Sharpen More, Find Edges, Unsharp Mask, Auto Levels, Levels, Curves, Invert): one column per
//! setting, three rows of test content: filter_probe <out.png>
//!
//! Rows: a colour-ramp ellipse with an anti-aliased edge on transparent black; the same with the
//! transparent pixels green (does their colour leak in?); an opaque ramp rectangle in 40..200
//! (edge handling at an object's border; room for Levels to stretch).
use firecraft_doc::{Bitmap, Document, EffectEntry, EffectParam, Element, ElementKind};
use firecraft_fwpng::Node;
use firecraft_raster::{Rgba, RgbaImage};

const W: u32 = 96;
const H: u32 = 72;

fn content(row: u32) -> RgbaImage {
    let mut img = RgbaImage::new(W, H, Rgba::TRANSPARENT);
    for y in 0..H {
        for x in 0..W {
            let check = (x / 12 + y / 12) % 2 == 0;
            let mut c = Rgba::new(
                (x * 255 / (W - 1)) as u8,
                (y * 255 / (H - 1)) as u8,
                if check { 30 } else { 230 },
                255,
            );
            if row == 2 {
                let s = |v: u8| (40 + v as u32 * 160 / 255) as u8;
                c = Rgba::new(s(c.r), s(c.g), s(c.b), 255);
            } else {
                // Ellipse with a 2-pixel soft edge.
                let (dx, dy) = ((x as f32 + 0.5 - W as f32 / 2.0) / (W as f32 / 2.0 - 2.0), (y as f32 + 0.5 - H as f32 / 2.0) / (H as f32 / 2.0 - 2.0));
                let d = (dx * dx + dy * dy).sqrt();
                let cover = ((1.0 - d) * (W as f32 / 4.0) + 0.5).clamp(0.0, 1.0);
                let a = (cover * 255.0).round() as u8;
                c.a = a;
                if a == 0 {
                    c = if row == 1 { Rgba::new(0, 255, 0, 0) } else { Rgba::TRANSPARENT };
                }
            }
            img.set(x as i64, y as i64, c);
        }
    }
    img
}

fn points(p: &[(f64, f64)]) -> EffectParam {
    EffectParam::Node(Node::vector(
        "GPL",
        p.iter()
            .map(|&(x, y)| Node::vector("GPT", vec![Node::float("XLC", x), Node::float("YLC", y)]))
            .collect(),
    ))
}

fn levels(ch: [[&str; 5]; 4]) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for (c, vals) in ["rgb", "red", "green", "blue"].iter().zip(ch) {
        for (k, val) in ["source_low", "source_high", "dest_low", "dest_high", "gamma"].iter().zip(vals) {
            v.push((format!("{k}_{c}"), val.to_string()));
        }
    }
    v
}

const ID: [&str; 5] = ["0", "255", "0", "255", "1"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: filter_probe <out.png>")?;
    // (label, moa, name, text params, curve points)
    type Curves = Vec<(&'static str, Vec<(f64, f64)>)>;
    let cols: Vec<(&str, &str, &str, Vec<(String, String)>, Curves)> = vec![
        ("none", "", "", vec![], vec![]),
        ("blur", "{f1cfce41-718e-11d1-8c8200a024cdc039}", "Blur", vec![], vec![]),
        ("blur more", "{f1cfce42-718e-11d1-8c8200a024cdc039}", "Blur More", vec![], vec![]),
        ("sharpen", "{c20952b1-fc76-11d0-8be700a024cdc039}", "Sharpen", vec![], vec![]),
        ("sharpen more", "{1f2f2591-9db7-11d1-8cad00a024cdc039}", "Sharpen More", vec![], vec![]),
        ("find edges", "{fc7093f1-f95c-11d0-8be200a024cdc039}", "Find Edges", vec![], vec![]),
        ("usm 1/50/0", "{f1cfce44-718e-11d1-8c8200a024cdc039}", "Unsharp Mask...",
            vec![("unsharp_mask_radius".into(), "1".into()), ("unsharp_mask_amount".into(), "50".into()), ("unsharp_mask_threshold".into(), "0".into())], vec![]),
        ("usm 4.1/150/0", "{f1cfce44-718e-11d1-8c8200a024cdc039}", "Unsharp Mask...",
            vec![("unsharp_mask_radius".into(), "4.1".into()), ("unsharp_mask_amount".into(), "150".into()), ("unsharp_mask_threshold".into(), "0".into())], vec![]),
        ("usm 2/200/20", "{f1cfce44-718e-11d1-8c8200a024cdc039}", "Unsharp Mask...",
            vec![("unsharp_mask_radius".into(), "2".into()), ("unsharp_mask_amount".into(), "200".into()), ("unsharp_mask_threshold".into(), "20".into())], vec![]),
        ("auto levels", "{d04ef8c1-71b4-11d1-8c8200a024cdc039}", "Auto Levels", vec![], vec![]),
        ("levels a", "{d04ef8c2-71b5-11d1-8c8200a024cdc039}", "Levels...",
            levels([["30", "220", "0", "255", "1"], ID, ID, ID]), vec![]),
        ("levels b", "{d04ef8c2-71b5-11d1-8c8200a024cdc039}", "Levels...",
            levels([["0", "255", "20", "240", "2"], ["0", "255", "50", "200", "0.5"], ID, ["60", "200", "0", "255", "1.5"]]), vec![]),
        ("curves a", "{3439b08e-1923-11d3-9bde00e02910d580}", "Curves...", vec![],
            vec![("rgb_points", vec![(0.0, 0.0), (64.0, 100.0), (192.0, 160.0), (255.0, 255.0)]),
                 ("red_points", vec![(0.0, 0.0), (255.0, 255.0)]), ("green_points", vec![(0.0, 0.0), (255.0, 255.0)]), ("blue_points", vec![(0.0, 0.0), (255.0, 255.0)])]),
        ("curves b", "{3439b08e-1923-11d3-9bde00e02910d580}", "Curves...", vec![],
            vec![("rgb_points", vec![(0.0, 0.0), (255.0, 255.0)]),
                 ("red_points", vec![(0.0, 30.0), (255.0, 220.0)]), ("green_points", vec![(0.0, 0.0), (128.0, 40.0), (255.0, 255.0)]),
                 ("blue_points", vec![(20.0, 255.0), (120.0, 10.0), (200.0, 200.0), (240.0, 0.0)])]),
        ("invert", "{d2541291-70d6-11d1-8c8000a024cdc039}", "Invert", vec![], vec![]),
    ];
    let (cw, ch) = (140u32, 110u32);
    let mut doc = Document::new(cw * cols.len() as u32, ch * 3, 72.0, None);
    for row in 0..3u32 {
        for (i, (label, moa, name, text, curves)) in cols.iter().enumerate() {
            let (x, y) = ((i as u32 * cw + 22) as i64, (row * ch + 19) as i64);
            let mut el = Element::new(ElementKind::Bitmap(Bitmap { x, y, image: content(row) }));
            el.name = Some(format!("{label} r{row}"));
            if !moa.is_empty() {
                let mut params: Vec<(String, EffectParam)> = vec![
                    ("EffectIsVisible".into(), EffectParam::Text("true".into())),
                    ("EffectMoaID".into(), EffectParam::Text((*moa).into())),
                    ("name".into(), EffectParam::Text((*name).into())),
                    ("MB_filter_preview_tile_size".into(), EffectParam::Text("-1 -1".into())),
                    ("mkbFile_WriteOnly_TemporaryEffectUiName".into(), EffectParam::Text((*name).into())),
                ];
                params.extend(text.iter().map(|(k, v)| (k.clone(), EffectParam::Text(v.clone()))));
                params.extend(curves.iter().map(|(k, p)| (k.to_string(), points(p))));
                el.effects.push(EffectEntry::new(params));
            }
            if let Some(v) = doc.layers[0].elements_mut(0) {
                v.push(el);
            }
        }
    }
    std::fs::write(&out, firecraft_io::save_fireworks_png(&doc)?)?;
    println!("{} objects -> {out}", cols.len() * 3);
    Ok(())
}
