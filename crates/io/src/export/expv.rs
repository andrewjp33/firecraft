//! Export settings in a Fireworks PNG: `MKBv > EXPv` (Fireworks MX's `dom.exportOptions`).
//!
//! Field codes measured with the oracle (each option set by script, the file saved, the record
//! compared): `FMTi` format (0 GIF, 1 JPEG, 2 PNG, 4 GIF animation), `CLRi` colour mode (0
//! indexed, 1 24 bit, 2 32 bit), `PLTi` palette (0 custom, 1 adaptive, 2 grid = uniform, 3
//! monochrome = grayscale, 4 Macintosh, 5 Windows, 6 exact, 7 Web 216), `PLRi` colours,
//! `PTNi` transparency (0 none, 1 index, 2 index alpha, 3 rgba), `APSb` WebSnap Adaptive,
//! `ASTi` its tolerance, `PLDi` dither percent, `GFIb` interlaced, `GLTi` GIF loss, `JPQi`
//! JPEG quality, `JPSi` smoothing, `JPPb` progressive, `FLPi` animation repeats (0 forever),
//! `FNLv` the frames. FireCraft's own options use `FC..` fields, which Fireworks ignores.

use firecraft_doc::{ExportFormat, ExportSettings, Frame, PaletteKind, Transparency};
use firecraft_fwpng::Node;
use firecraft_raster::Rgba;

/// FireCraft's format code for formats Fireworks doesn't have (`FCFi`).
const FC_WEBP: i64 = 1;
const FC_JPEG_XL: i64 = 2;

pub(crate) fn read(exp: &Node) -> ExportSettings {
    let d = ExportSettings::default();
    let int = |t: &str| exp.int_of(t);
    let byte = |t: &str, def: u8| int(t).map_or(def, |v| v.clamp(0, 255) as u8);
    let mut format = match (int("FMT"), int("CLR")) {
        (Some(1), _) => ExportFormat::Jpeg,
        (Some(2), Some(1)) => ExportFormat::Png24,
        (Some(2), Some(2)) => ExportFormat::Png32,
        (Some(2), _) => ExportFormat::Png8,
        (Some(4), _) => ExportFormat::AnimatedGif,
        _ => ExportFormat::Gif,
    };
    match int("FCF") {
        Some(FC_WEBP) => format = ExportFormat::WebP,
        Some(FC_JPEG_XL) => format = ExportFormat::JpegXl,
        _ => {}
    }
    let colors = int("PLR").map_or(d.colors, |v| v.clamp(2, 256) as u16);
    let snap = exp.bool_of("APS").unwrap_or(true);
    let palette = match int("PLT") {
        Some(2) => PaletteKind::Uniform,
        Some(3) if colors <= 2 => PaletteKind::BlackWhite,
        Some(3) => PaletteKind::Grayscale,
        Some(4) => PaletteKind::Macintosh,
        Some(5) => PaletteKind::Windows,
        Some(6) => PaletteKind::Exact,
        Some(7) => PaletteKind::Web216,
        // Custom palettes (0) aren't modelled yet: they export as Adaptive.
        Some(1) | None if snap => PaletteKind::WebAdaptive,
        _ => PaletteKind::Adaptive,
    };
    let transparency = match int("PTN") {
        Some(1) => Transparency::Index,
        Some(2) | Some(3) => Transparency::Alpha,
        _ => Transparency::None,
    };
    ExportSettings {
        format,
        palette,
        colors,
        websnap_tolerance: byte("AST", d.websnap_tolerance),
        dither: byte("PLD", 0).min(100),
        transparency,
        matte: int("FCM").map(|v| {
            let v = v as u32;
            Rgba::new((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
        }),
        interlaced: exp.bool_of("GFI").unwrap_or(false),
        loops: int("FLP").map_or(0, |v| v.clamp(0, 65535) as u16),
        jpeg_quality: byte("JPQ", d.jpeg_quality).min(100),
        jpeg_smoothing: byte("JPS", 0).min(8),
        progressive: exp.bool_of("JPP").unwrap_or(false),
        jpeg_full_color: exp.bool_of("FCJ").unwrap_or(true),
        quality: byte("FCQ", d.quality).min(100),
        alpha_quality: byte("FCA", d.alpha_quality).min(100),
        lossless: exp.bool_of("FCL").unwrap_or(false),
        raw: Some(exp.clone()),
    }
}

/// Whether `s` is what `raw` already says (then the record is kept exactly as Fireworks wrote
/// it, custom palettes and all).
pub(crate) fn unchanged(raw: &Node, s: &ExportSettings) -> bool {
    let mut a = read(raw);
    a.raw = None;
    let mut b = s.clone();
    b.raw = None;
    a == b
}

/// The record for `s`: the one read from the file with FireCraft's settings written over it,
/// or Fireworks MX's default record for a new document. `frames` fill `FNLv`.
pub(crate) fn write(s: &ExportSettings, frames: &[Frame]) -> Node {
    let mut n = s.raw.clone().unwrap_or_else(template);
    let (fmt, clr) = match s.format {
        ExportFormat::Gif => (0, 0),
        ExportFormat::Jpeg => (1, 1),
        ExportFormat::Png8 => (2, 0),
        ExportFormat::Png24 => (2, 1),
        // Fireworks MX exports FireCraft's own formats as PNG 32.
        ExportFormat::Png32 | ExportFormat::WebP | ExportFormat::JpegXl => (2, 2),
        ExportFormat::AnimatedGif => (4, 0),
    };
    let plt = match s.palette {
        PaletteKind::Adaptive | PaletteKind::WebAdaptive => 1,
        PaletteKind::Uniform => 2,
        PaletteKind::Grayscale | PaletteKind::BlackWhite => 3,
        PaletteKind::Macintosh => 4,
        PaletteKind::Windows => 5,
        PaletteKind::Exact => 6,
        PaletteKind::Web216 => 7,
    };
    let colors = if s.palette == PaletteKind::BlackWhite {
        2
    } else {
        s.colors.clamp(2, 256)
    };
    let ptn = match s.transparency {
        Transparency::None => 0,
        Transparency::Index => 1,
        Transparency::Alpha if s.format == ExportFormat::Png8 => 3,
        Transparency::Alpha => 2,
    };
    set(&mut n, Node::int("FMT", fmt));
    set(&mut n, Node::int("CLR", clr));
    set(&mut n, Node::int("PLT", plt));
    set(&mut n, Node::int("PLR", i64::from(colors)));
    set(&mut n, Node::int("PTN", ptn));
    set(&mut n, Node::bool("APS", s.palette == PaletteKind::WebAdaptive));
    set(&mut n, Node::int("AST", i64::from(s.websnap_tolerance)));
    set(&mut n, Node::int("PLD", i64::from(s.dither.min(100))));
    set(&mut n, Node::int("JPQ", i64::from(s.jpeg_quality.min(100))));
    set(&mut n, Node::int("JPS", i64::from(s.jpeg_smoothing.min(8))));
    set(&mut n, Node::bool("JPP", s.progressive));
    set(&mut n, Node::bool("GFI", s.interlaced));
    set(&mut n, Node::int("FLP", i64::from(s.loops)));
    // FireCraft's own fields, at the end (Fireworks ignores fields it doesn't know).
    n.children.retain(|c| !["FCF", "FCQ", "FCA", "FCL", "FCJ", "FCM"].iter().any(|t| c.is(t)));
    match s.format {
        ExportFormat::WebP => n.children.push(Node::int("FCF", FC_WEBP)),
        ExportFormat::JpegXl => n.children.push(Node::int("FCF", FC_JPEG_XL)),
        _ => {}
    }
    n.children.push(Node::int("FCQ", i64::from(s.quality.min(100))));
    n.children.push(Node::int("FCA", i64::from(s.alpha_quality.min(100))));
    n.children.push(Node::bool("FCL", s.lossless));
    n.children.push(Node::bool("FCJ", s.jpeg_full_color));
    if let Some(m) = s.matte {
        let v = (u32::from(m.r) << 16) | (u32::from(m.g) << 8) | u32::from(m.b);
        n.children.push(Node::int("FCM", i64::from(v)));
    }
    write_frames(&mut n, frames);
    n
}

/// `FNLv`: one `FNMv{FNPi, FNDi, VISb}` per frame, after `FLPi` as Fireworks writes it. Frames
/// already recorded keep their other fields.
fn write_frames(n: &mut Node, frames: &[Frame]) {
    if n.child("FNL").is_none() {
        let at = n
            .children
            .iter()
            .position(|c| c.is("FLP"))
            .map_or(n.children.len(), |i| i + 1);
        n.children.insert(at, Node::vector("FNL", Vec::new()));
    }
    let Some(fnl) = n.child_mut("FNL") else { return };
    let mut old = fnl.children.iter().filter(|c| c.is("FNM")).cloned();
    let list: Vec<Node> = frames
        .iter()
        .map(|f| {
            let mut m = old.next().unwrap_or_else(|| Node::vector("FNM", vec![Node::int("FNP", 0)]));
            set(&mut m, Node::int("FND", i64::from(f.delay)));
            set(&mut m, Node::bool("VIS", f.export));
            m
        })
        .collect();
    fnl.children = list;
}

fn set(parent: &mut Node, node: Node) {
    match parent.children.iter_mut().find(|c| c.tag == node.tag) {
        Some(slot) => *slot = node,
        None => parent.children.push(node),
    }
}

/// Fireworks MX's record for a new document, field for field (read from a file it saved).
fn template() -> Node {
    let i = Node::int;
    let b = Node::bool;
    Node::vector(
        "EXP",
        vec![
            i("FMT", 0),
            i("CLR", 0),
            i("PLT", 1),
            Node::vector("PAL", Vec::new()),
            i("PLN", 0),
            i("PLR", 128),
            i("PTN", 0),
            b("PLO", true),
            b("APL", true),
            b("APS", true),
            b("AAC", true),
            b("AAD", true),
            i("AST", 14),
            i("PLD", 0),
            i("GLT", 0),
            i("JPQ", 80),
            i("JPS", 0),
            i("JPO", 0),
            b("JPP", false),
            b("GFI", false),
            i("JSQ", 90),
            b("JSE", false),
            b("JST", true),
            b("JSB", false),
            i("FLP", 0),
            b("GSL", false),
            b("LSU", false),
            b("DSU", false),
            b("LSD", false),
            b("NYS", true),
            i("LSS", 1),
            i("HTM", 0),
            Node::string("HTT", "Dreamweaver"),
            i("XFS", 0),
            i("XMD", 0),
            b("TRM", true),
            b("HTM", true),
            b("XDA", true),
            Node::vector("NM4", vec![i("NM4", 0)]),
            Node::vector("NMF", vec![i("NMF", 1)]),
            i("TAL", 0),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_the_record() {
        let frames = vec![Frame::default(), Frame { delay: 50, ..Frame::default() }];
        for s in [
            ExportSettings::default(),
            ExportSettings {
                format: ExportFormat::Png8,
                palette: PaletteKind::Exact,
                colors: 64,
                dither: 37,
                transparency: Transparency::Alpha,
                interlaced: true,
                ..ExportSettings::default()
            },
            ExportSettings {
                format: ExportFormat::Jpeg,
                jpeg_quality: 63,
                jpeg_smoothing: 3,
                progressive: true,
                matte: Some(Rgba::new(1, 2, 3, 255)),
                ..ExportSettings::default()
            },
            ExportSettings {
                format: ExportFormat::JpegXl,
                quality: 70,
                alpha_quality: 40,
                lossless: true,
                ..ExportSettings::default()
            },
            ExportSettings {
                format: ExportFormat::AnimatedGif,
                palette: PaletteKind::BlackWhite,
                colors: 2,
                loops: 3,
                ..ExportSettings::default()
            },
        ] {
            let node = write(&s, &frames);
            let back = read(&node);
            assert!(back.same_effect(&s), "{s:?} came back as {back:?}");
            assert_eq!(back.matte, s.matte);
            let fnl = node.child("FNL").expect("frames");
            assert_eq!(fnl.children.len(), 2);
        }
    }

    #[test]
    fn webp_falls_back_to_png_32_in_fireworks() {
        let s = ExportSettings {
            format: ExportFormat::WebP,
            ..ExportSettings::default()
        };
        let n = write(&s, &[Frame::default()]);
        assert_eq!((n.int_of("FMT"), n.int_of("CLR"), n.int_of("FCF")), (Some(2), Some(2), Some(1)));
    }
}
