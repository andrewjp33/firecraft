//! Colour palettes and quantization for indexed exports (GIF, PNG 8): Fireworks MX's palette
//! kinds, the nearest-colour mapping and diffusion dithering.
//!
//! The fixed palettes (Windows, Macintosh) are Fireworks MX's own, read from GIFs it exported
//! (docs/behavior/export.md). The adaptive palette is a weighted median cut; Fireworks MX's own
//! adaptive algorithm isn't reproduced yet.

use std::collections::HashMap;

/// A palette entry, RGBA.
pub type Entry = [u8; 4];

const WINDOWS: &str = include_str!("palettes/windows.txt");
const MACINTOSH: &str = include_str!("palettes/macintosh.txt");

fn parse(table: &str) -> Vec<Entry> {
    table
        .lines()
        .filter_map(|l| u32::from_str_radix(l.trim(), 16).ok())
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        .collect()
}

/// Fireworks MX's Windows palette (256 entries).
pub fn windows() -> Vec<Entry> {
    parse(WINDOWS)
}

/// Fireworks MX's Macintosh palette (256 entries).
pub fn macintosh() -> Vec<Entry> {
    parse(MACINTOSH)
}

/// The 216 web-safe colours, in Fireworks' order (white first, black last).
pub fn web216() -> Vec<Entry> {
    let mut out = Vec::with_capacity(216);
    for r in (0..6).rev() {
        for g in (0..6).rev() {
            for b in (0..6).rev() {
                out.push([r * 51, g * 51, b * 51, 255]);
            }
        }
    }
    out
}

/// A uniform grid of colours with at most `n` entries (`levels³ ≤ n`; 256 gives the 216 cube).
pub fn uniform(n: usize) -> Vec<Entry> {
    let mut levels = 1usize;
    while (levels + 1).pow(3) <= n.max(1) {
        levels += 1;
    }
    let step = |i: usize| -> u8 {
        if levels <= 1 {
            0
        } else {
            ((i * 255 + (levels - 1) / 2) / (levels - 1)) as u8
        }
    };
    let mut out = Vec::new();
    for r in (0..levels).rev() {
        for g in (0..levels).rev() {
            for b in (0..levels).rev() {
                out.push([step(r), step(g), step(b), 255]);
            }
        }
    }
    out
}

/// `n` greys from white to black, as Fireworks MX's grayscale ("monochrome") palette: entry i
/// is round(255 · (n − 1 − i) / (n − 1)).
pub fn grayscale(n: usize) -> Vec<Entry> {
    let n = n.clamp(2, 256);
    (0..n)
        .map(|i| {
            let v = ((255 * (n - 1 - i)) as f64 / (n - 1) as f64).round() as u8;
            [v, v, v, 255]
        })
        .collect()
}

fn key(c: Entry) -> u32 {
    u32::from_be_bytes(c)
}

fn unkey(k: u32) -> Entry {
    k.to_be_bytes()
}

/// How often each colour occurs.
pub fn histogram(pixels: &[Entry]) -> HashMap<u32, u32> {
    let mut h = HashMap::new();
    for &p in pixels {
        *h.entry(key(p)).or_insert(0) += 1;
    }
    h
}

/// An adaptive palette of at most `n` colours: the colours themselves when there are few
/// enough, otherwise a weighted median cut. `alpha`: alpha takes part as a fourth channel.
pub fn adaptive(hist: &HashMap<u32, u32>, n: usize, alpha: bool) -> Vec<Entry> {
    let n = n.max(1);
    let mut colors: Vec<(Entry, u32)> = hist.iter().map(|(&k, &c)| (unkey(k), c)).collect();
    if colors.len() <= n {
        colors.sort_by_key(|c| std::cmp::Reverse(c.1));
        return colors.into_iter().map(|c| c.0).collect();
    }
    let channels = if alpha { 4 } else { 3 };
    let mut boxes: Vec<Vec<(Entry, u32)>> = vec![colors];
    while boxes.len() < n {
        // Split the box whose widest channel range, weighted by its pixel count, is largest.
        let pick = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let (ch, range) = widest(b, channels);
                let weight: u64 = b.iter().map(|c| u64::from(c.1)).sum();
                (i, ch, u64::from(range) * (weight as f64).sqrt() as u64)
            })
            .max_by_key(|t| t.2);
        let Some((i, ch, score)) = pick else { break };
        if score == 0 {
            break;
        }
        let mut b = boxes.swap_remove(i);
        b.sort_unstable_by_key(|c| c.0.get(ch).copied().unwrap_or(0));
        let total: u64 = b.iter().map(|c| u64::from(c.1)).sum();
        let mut acc = 0u64;
        let mut cut = 1;
        for (j, c) in b.iter().enumerate() {
            acc += u64::from(c.1);
            if acc * 2 >= total {
                cut = (j + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(cut);
        boxes.push(b);
        boxes.push(rest);
    }
    let mut out: Vec<(Entry, u64)> = boxes
        .iter()
        .map(|b| {
            let w: u64 = b.iter().map(|c| u64::from(c.1)).sum::<u64>().max(1);
            let mut sum = [0u64; 4];
            for (c, n) in b {
                for (s, &v) in sum.iter_mut().zip(c) {
                    *s += u64::from(v) * u64::from(*n);
                }
            }
            let avg = |k: usize| ((sum.get(k).copied().unwrap_or(0) + w / 2) / w).min(255) as u8;
            ([avg(0), avg(1), avg(2), if alpha { avg(3) } else { 255 }], w)
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.1));
    out.into_iter().map(|c| c.0).collect()
}

fn widest(b: &[(Entry, u32)], channels: usize) -> (usize, u8) {
    (0..channels)
        .map(|k| {
            let at = |c: &(Entry, u32)| c.0.get(k).copied().unwrap_or(0);
            let lo = b.iter().map(at).min().unwrap_or(0);
            let hi = b.iter().map(at).max().unwrap_or(0);
            (k, hi - lo)
        })
        .max_by_key(|t| t.1)
        .unwrap_or((0, 0))
}

/// WebSnap: palette colours within `tolerance` (every channel) of a web-safe colour become it.
pub fn web_snap(palette: &mut [Entry], tolerance: u8) {
    for c in palette.iter_mut() {
        let snapped: Vec<u8> = c[..3]
            .iter()
            .map(|&v| (((u16::from(v) + 25) / 51) * 51) as u8)
            .collect();
        if c[..3]
            .iter()
            .zip(&snapped)
            .all(|(&a, &b)| a.abs_diff(b) <= tolerance)
        {
            c[..3].copy_from_slice(&snapped);
        }
    }
}

/// The `n` entries of a fixed palette the pixels use most (Fireworks MX trims Windows,
/// Macintosh and Web 216 to the colour count this way), in the palette's order.
pub fn most_used(palette: &[Entry], hist: &HashMap<u32, u32>, n: usize) -> Vec<Entry> {
    if palette.len() <= n {
        return palette.to_vec();
    }
    let mut use_count = vec![0u64; palette.len()];
    for (&k, &c) in hist {
        let i = nearest(palette, unkey(k), false);
        if let Some(u) = use_count.get_mut(i) {
            *u += u64::from(c);
        }
    }
    let mut order: Vec<usize> = (0..palette.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(use_count.get(i).copied().unwrap_or(0)));
    let mut keep: Vec<usize> = order.into_iter().take(n).collect();
    keep.sort_unstable();
    keep.into_iter().filter_map(|i| palette.get(i).copied()).collect()
}

/// The index of the palette entry nearest `c` (squared RGB distance, plus alpha when `alpha`).
pub fn nearest(palette: &[Entry], c: Entry, alpha: bool) -> usize {
    let mut best = (u32::MAX, 0usize);
    for (i, p) in palette.iter().enumerate() {
        // Plain RGB distance: Fireworks MX maps a colour cube to its Windows, Macintosh and
        // Web 216 palettes this way (99.4-100% of pixels; weighted distances fit worse).
        let channels = if alpha { 4 } else { 3 };
        let dist: u32 = p
            .iter()
            .zip(&c)
            .take(channels)
            .map(|(&a, &b)| {
                let v = i32::from(a) - i32::from(b);
                (v * v) as u32
            })
            .sum();
        if dist < best.0 {
            best = (dist, i);
            if dist == 0 {
                break;
            }
        }
    }
    best.1
}

/// Map `pixels` (`width` wide) to palette indices. Pixels for which `skip` is true get
/// `skip_index` (a transparent entry). `dither`: Floyd-Steinberg diffusion, 0-100 percent.
pub fn map(
    pixels: &[Entry],
    width: usize,
    palette: &[Entry],
    dither: u8,
    alpha: bool,
    skip: &dyn Fn(Entry) -> bool,
    skip_index: u8,
) -> Vec<u8> {
    let mut cache: HashMap<u32, u8> = HashMap::new();
    let mut lookup = |c: Entry| -> u8 {
        *cache
            .entry(key(c))
            .or_insert_with(|| nearest(palette, c, alpha).min(255) as u8)
    };
    let width = width.max(1);
    if dither == 0 || palette.is_empty() {
        return pixels
            .iter()
            .map(|&p| if skip(p) { skip_index } else { lookup(p) })
            .collect();
    }
    let amount = f32::from(dither.min(100)) / 100.0;
    let height = pixels.len() / width;
    let mut out = vec![0u8; pixels.len()];
    // Error carried to this row and the next, per channel.
    let mut cur = vec![[0f32; 4]; width + 2];
    let mut next = vec![[0f32; 4]; width + 2];
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let Some(&p) = pixels.get(i) else { continue };
            if skip(p) {
                if let Some(o) = out.get_mut(i) {
                    *o = skip_index;
                }
                continue;
            }
            let e = cur.get(x + 1).copied().unwrap_or_default();
            let mut want = p;
            for ((w, &v), &d) in want.iter_mut().zip(&p).zip(&e) {
                *w = (f32::from(v) + d).round().clamp(0.0, 255.0) as u8;
            }
            if !alpha {
                want[3] = p[3];
            }
            let idx = lookup(want);
            if let Some(o) = out.get_mut(i) {
                *o = idx;
            }
            let got = palette.get(idx as usize).copied().unwrap_or(want);
            let mut err = [0f32; 4];
            for ((e, &w), &g) in err.iter_mut().zip(&want).zip(&got) {
                *e = (f32::from(w) - f32::from(g)) * amount;
            }
            let add = |row: &mut Vec<[f32; 4]>, at: usize, w: f32| {
                if let Some(c) = row.get_mut(at) {
                    for (v, &e) in c.iter_mut().zip(&err) {
                        *v += e * w;
                    }
                }
            };
            add(&mut cur, x + 2, 7.0 / 16.0);
            add(&mut next, x, 3.0 / 16.0);
            add(&mut next, x + 1, 5.0 / 16.0);
            add(&mut next, x + 2, 1.0 / 16.0);
        }
        std::mem::swap(&mut cur, &mut next);
        next.iter_mut().for_each(|c| *c = [0.0; 4]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_palettes_have_fireworks_sizes() {
        assert_eq!(windows().len(), 256);
        assert_eq!(macintosh().len(), 256);
        assert_eq!(web216().len(), 216);
        assert_eq!(uniform(256).len(), 216);
        assert_eq!(uniform(8).len(), 8);
        // Fireworks MX's 128-grey palette: ..., 818181 (63), 7e7e7e (64), ...
        let g = grayscale(128);
        assert_eq!((g[63][0], g[64][0], g[0][0], g[127][0]), (0x81, 0x7e, 0xff, 0));
    }

    #[test]
    fn few_colours_are_kept_exactly() {
        let px = [[255, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255]];
        let pal = adaptive(&histogram(&px), 16, false);
        assert_eq!(pal, vec![[255, 0, 0, 255], [0, 0, 255, 255]]);
        let idx = map(&px, 3, &pal, 0, false, &|_| false, 0);
        assert_eq!(idx, vec![0, 1, 0]);
    }

    #[test]
    fn median_cut_spans_a_gradient() {
        let px: Vec<Entry> = (0..=255u8).map(|v| [v, v, v, 255]).collect();
        let pal = adaptive(&histogram(&px), 4, false);
        assert_eq!(pal.len(), 4);
        let mut lum: Vec<u8> = pal.iter().map(|c| c[0]).collect();
        lum.sort_unstable();
        assert!(lum[0] < 40 && lum[3] > 215, "{lum:?}");
    }

    #[test]
    fn websnap_moves_only_near_colours() {
        let mut pal = vec![[0x34, 0x65, 0x99, 255], [0x45, 0x65, 0x99, 255]];
        web_snap(&mut pal, 14);
        assert_eq!(pal[0], [0x33, 0x66, 0x99, 255]);
        assert_eq!(pal[1], [0x45, 0x65, 0x99, 255]);
    }

    #[test]
    fn dithering_mixes_two_colours_for_a_grey() {
        let px = vec![[128, 128, 128, 255]; 64];
        let pal = vec![[0, 0, 0, 255], [255, 255, 255, 255]];
        let idx = map(&px, 8, &pal, 100, false, &|_| false, 0);
        let whites = idx.iter().filter(|&&i| i == 1).count();
        assert!((24..=40).contains(&whites), "{whites}");
        assert!(map(&px, 8, &pal, 0, false, &|_| false, 0).iter().all(|&i| i == idx[0]));
    }
}
