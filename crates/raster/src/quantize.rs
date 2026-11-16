//! Colour palettes and quantization for indexed exports (GIF, PNG 8): Fireworks MX's palette
//! kinds, the nearest-colour mapping and diffusion dithering.
//!
//! The fixed palettes (Windows, Macintosh) are Fireworks MX's own, read from GIFs it exported
//! (docs/behavior/export.md). The adaptive palette is a weighted median cut; Fireworks MX's own
//! adaptive algorithm isn't reproduced yet.

use std::hash::{BuildHasherDefault, Hasher};

/// Colour tables are keyed by 32-bit colours: a multiplicative hash is several times faster
/// than the default (SipHash) on the millions of pixels of an animation (tubes.png: 26 million).
pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<ColorHasher>>;

/// Fibonacci hashing of small integer keys (no protection against crafted keys, which colours
/// don't need).
#[derive(Default, Clone, Copy)]
pub struct ColorHasher(u64);

impl Hasher for ColorHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u16(&mut self, n: u16) {
        self.write_u64(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn finish(&self) -> u64 {
        // The high bits are the well mixed ones; fold them into the low ones the table uses too.
        self.0 ^ (self.0 >> 29)
    }
}

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

/// A uniform grid fitted to these colours, at most `n` entries: each channel's range (lowest to
/// highest value) cut into equal slices with a level in the middle of each, so no value is
/// further than half a slice from one (two levels over 0..255 are 64 and 191, not 0 and 255);
/// the channels are given levels one at a time, always to the one with the coarsest step, while
/// the grid still fits (so 256 colours can be 10 x 25 rather than the 6-level cube). Fully
/// opaque colours only.
pub fn uniform_fitted(hist: &HashMap<u32, u32>, n: usize) -> Vec<Entry> {
    let n = n.max(1);
    let (mut lo, mut hi) = ([255u8; 3], [0u8; 3]);
    for &k in hist.keys() {
        let c = unkey(k);
        for ch in 0..3 {
            if let (Some(l), Some(h), Some(&v)) = (lo.get_mut(ch), hi.get_mut(ch), c.get(ch)) {
                (*l, *h) = ((*l).min(v), (*h).max(v));
            }
        }
    }
    if hist.is_empty() {
        return vec![[0, 0, 0, 255]];
    }
    let range: [f64; 3] = std::array::from_fn(|ch| {
        f64::from(hi.get(ch).copied().unwrap_or(0)) - f64::from(lo.get(ch).copied().unwrap_or(0))
    });
    // The largest distance from a value to its nearest level (levels at the middles of equal
    // slices of the range).
    let reach = |r: f64, l: usize| r / (2.0 * l.max(1) as f64);
    let mut levels = [1usize; 3];
    loop {
        let mut order = [0usize, 1, 2];
        order.sort_by(|&a, &b| {
            let ea = reach(range.get(a).copied().unwrap_or(0.0), levels.get(a).copied().unwrap_or(1));
            let eb = reach(range.get(b).copied().unwrap_or(0.0), levels.get(b).copied().unwrap_or(1));
            eb.total_cmp(&ea)
        });
        let grown = order.into_iter().find(|&ch| {
            let r = range.get(ch).copied().unwrap_or(0.0);
            let mut more = levels;
            if let Some(l) = more.get_mut(ch) {
                *l += 1;
            }
            // A level per distinct value at most.
            r > 0.0 && levels.get(ch).is_some_and(|&l| (l as f64) <= r) && more.iter().product::<usize>() <= n
        });
        match grown.and_then(|ch| levels.get_mut(ch)) {
            Some(l) => *l += 1,
            None => break,
        }
    }
    let values: [Vec<u8>; 3] = std::array::from_fn(|ch| {
        let (l, lo) = (levels.get(ch).copied().unwrap_or(1), f64::from(lo.get(ch).copied().unwrap_or(0)));
        let r = range.get(ch).copied().unwrap_or(0.0);
        (0..l)
            .map(|i| {
                let v = lo + r * (i as f64 + 0.5) / l as f64;
                v.round().clamp(0.0, 255.0) as u8
            })
            .collect()
    });
    let [rs, gs, bs] = &values;
    let mut out = Vec::with_capacity(rs.len() * gs.len() * bs.len());
    for &r in rs {
        for &g in gs {
            for &b in bs {
                out.push([r, g, b, 255]);
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
    let mut h = HashMap::default();
    for &p in pixels {
        *h.entry(key(p)).or_insert(0) += 1;
    }
    h
}

/// The `n` greys that fit grey pixels best (least squared error): the sorted levels split into
/// `n` runs by dynamic programming, each run's weighted mean its grey. Exact, unlike a median
/// cut, which leaves rare light or dark greys far from any entry. Pixels must be grey already
/// (r = g = b; only r is read).
pub fn grays_best_fit(hist: &HashMap<u32, u32>, n: usize) -> Vec<Entry> {
    let n = n.max(1);
    let mut count = [0u64; 256];
    for (&k, &c) in hist {
        if let Some(t) = count.get_mut(usize::from(unkey(k)[0])) {
            *t += u64::from(c);
        }
    }
    let levels: Vec<(f64, f64)> = count
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c > 0)
        .map(|(v, &c)| (v as f64, c as f64))
        .collect();
    let m = levels.len();
    let grey = |v: f64| {
        let g = v.round().clamp(0.0, 255.0) as u8;
        [g, g, g, 255]
    };
    if m <= n {
        return levels.iter().map(|&(v, _)| grey(v)).collect();
    }
    // Prefix sums of weight, weight*v, weight*v^2: the squared error of levels i..j about their
    // mean in O(1).
    let prefix: Vec<(f64, f64, f64)> = std::iter::once((0.0, 0.0, 0.0))
        .chain(levels.iter().scan((0.0, 0.0, 0.0), |acc: &mut (f64, f64, f64), &(v, c)| {
            *acc = (acc.0 + c, acc.1 + c * v, acc.2 + c * v * v);
            Some(*acc)
        }))
        .collect();
    let sums = |i: usize, j: usize| {
        let (a, b) = (prefix.get(i).copied().unwrap_or_default(), prefix.get(j).copied().unwrap_or_default());
        (b.0 - a.0, b.1 - a.1, b.2 - a.2)
    };
    let cost = |i: usize, j: usize| {
        let (cw, c1, c2) = sums(i, j);
        if cw > 0.0 { c2 - c1 * c1 / cw } else { 0.0 }
    };
    // Row k: the least error of the first j levels in k runs; `from` where each last run starts.
    let mut prev = vec![f64::INFINITY; m + 1];
    if let Some(p) = prev.first_mut() {
        *p = 0.0;
    }
    let mut from: Vec<Vec<usize>> = Vec::with_capacity(n);
    for k in 1..=n {
        let mut cur = vec![f64::INFINITY; m + 1];
        let mut start = vec![0usize; m + 1];
        for j in k..=m {
            let (mut least, mut arg) = (f64::INFINITY, k - 1);
            for i in (k - 1)..j {
                let e = prev.get(i).copied().unwrap_or(f64::INFINITY) + cost(i, j);
                if e < least {
                    (least, arg) = (e, i);
                }
            }
            if let (Some(c), Some(a)) = (cur.get_mut(j), start.get_mut(j)) {
                (*c, *a) = (least, arg);
            }
        }
        from.push(start);
        prev = cur;
    }
    let mut out = Vec::with_capacity(n);
    let mut j = m;
    for start in from.iter().rev() {
        let i = start.get(j).copied().unwrap_or(0);
        let (cw, c1, _) = sums(i, j);
        out.push(grey(c1 / cw.max(1.0)));
        j = i;
    }
    out.reverse();
    out.dedup();
    out
}

/// An adaptive palette of at most `n` colours: the colours themselves when there are few
/// enough, otherwise a weighted median cut. `alpha`: alpha takes part as a fourth channel.
pub fn adaptive(hist: &HashMap<u32, u32>, n: usize, alpha: bool) -> Vec<Entry> {
    let n = n.max(1);
    // In a fixed order (a hash map's order varies between runs), so an export is the same every time.
    let mut colors: Vec<(Entry, u32)> = hist.iter().map(|(&k, &c)| (unkey(k), c)).collect();
    colors.sort_unstable_by_key(|c| key(c.0));
    if colors.len() <= n {
        colors.sort_by_key(|c| std::cmp::Reverse(c.1));
        return colors.into_iter().map(|c| c.0).collect();
    }
    // Millions of colours (an animation of blurred frames: tubes.png has 5.8 million): the
    // median cut works on them grouped by 5 bits per channel (each group its pixels' mean
    // colour). Fewer colours, as in almost every image, are cut exactly as they are.
    if colors.len() > GROUP_ABOVE && !alpha {
        colors = grouped(&colors);
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

/// The `n` entries of a fixed palette that fit the pixels best: chosen one at a time, each time
/// the entry that lowers the total squared error the most (a greedy pick, close to the best
/// possible subset). Unlike [`most_used`] it keeps an entry that only a few pixels need when
/// nothing else is near them. The pixels are grouped by 4 bits per channel (their mean colour)
/// for the choice; the gains are re-evaluated lazily (they only shrink as entries are added).
/// In the palette's order.
pub fn best_subset(palette: &[Entry], hist: &HashMap<u32, u32>, n: usize) -> Vec<Entry> {
    if palette.len() <= n {
        return palette.to_vec();
    }
    // Groups of similar colours: weight and summed channels.
    let mut groups: HashMap<u16, (u64, [u64; 3])> = HashMap::default();
    for (&k, &c) in hist {
        let e = unkey(k);
        let cell = (u16::from(e[0] >> 4) << 8) | (u16::from(e[1] >> 4) << 4) | u16::from(e[2] >> 4);
        let g = groups.entry(cell).or_insert((0, [0; 3]));
        g.0 += u64::from(c);
        for (sum, &v) in g.1.iter_mut().zip(&e) {
            *sum += u64::from(v) * u64::from(c);
        }
    }
    let mut cells: Vec<(u16, (u64, [u64; 3]))> = groups.into_iter().collect();
    cells.sort_unstable_by_key(|c| c.0);
    let points: Vec<(u64, [i64; 3])> = cells
        .iter()
        .map(|&(_, (w, s))| (w, s.map(|v| (v / w.max(1)) as i64)))
        .collect();
    let dist = |p: &[i64; 3], e: &Entry| -> u64 {
        p.iter()
            .zip(e)
            .map(|(&a, &b)| {
                let d = a - i64::from(b);
                (d * d) as u64
            })
            .sum()
    };
    // Each point's error with the entries chosen so far (none yet: unbounded).
    let mut current: Vec<u64> = vec![u64::MAX; points.len()];
    let gain = |current: &[u64], e: &Entry| -> u64 {
        points
            .iter()
            .zip(current)
            .map(|((w, p), &now)| {
                let d = dist(p, e);
                if now == u64::MAX {
                    // Counted against a large error, so the first pick covers the most.
                    w * (1u64 << 20).saturating_sub(d)
                } else {
                    w * now.saturating_sub(d)
                }
            })
            .sum()
    };
    let mut chosen = vec![false; palette.len()];
    // Upper bounds of each entry's gain (lazy greedy).
    let mut bound: Vec<u64> = palette.iter().map(|e| gain(&current, e)).collect();
    for _ in 0..n {
        let pick = loop {
            let Some((i, _)) = bound
                .iter()
                .enumerate()
                .filter(|&(i, _)| !chosen.get(i).copied().unwrap_or(true))
                .max_by_key(|&(i, &b)| (b, std::cmp::Reverse(i)))
            else {
                break None;
            };
            let fresh = palette.get(i).map_or(0, |e| gain(&current, e));
            let next = bound
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i && !chosen.get(j).copied().unwrap_or(true))
                .map(|(_, &b)| b)
                .max()
                .unwrap_or(0);
            if let Some(b) = bound.get_mut(i) {
                *b = fresh;
            }
            if fresh >= next {
                break Some(i);
            }
        };
        let Some(i) = pick else { break };
        if let Some(c) = chosen.get_mut(i) {
            *c = true;
        }
        if let Some(e) = palette.get(i) {
            for (now, (_, p)) in current.iter_mut().zip(&points) {
                *now = (*now).min(dist(p, e));
            }
        }
    }
    palette
        .iter()
        .zip(&chosen)
        .filter(|&(_, &c)| c)
        .map(|(&e, _)| e)
        .collect()
}

/// Above this many distinct colours [`adaptive`] groups them first.
const GROUP_ABOVE: usize = 1 << 18;

/// Colours grouped by 5 bits per channel: each group's weighted mean colour and total weight,
/// in a fixed order.
fn grouped(colors: &[(Entry, u32)]) -> Vec<(Entry, u32)> {
    let mut groups: HashMap<u16, (u64, [u64; 3])> = HashMap::default();
    for &(c, w) in colors {
        let cell = (u16::from(c[0] >> 3) << 10) | (u16::from(c[1] >> 3) << 5) | u16::from(c[2] >> 3);
        let g = groups.entry(cell).or_insert((0, [0; 3]));
        g.0 += u64::from(w);
        for (sum, &v) in g.1.iter_mut().zip(&c) {
            *sum += u64::from(v) * u64::from(w);
        }
    }
    let mut out: Vec<(Entry, u32)> = groups
        .into_values()
        .map(|(w, s)| {
            let m = s.map(|v| (v / w.max(1)).min(255) as u8);
            ([m[0], m[1], m[2], 255], w.min(u64::from(u32::MAX)) as u32)
        })
        .collect();
    out.sort_unstable_by_key(|c| (key(c.0), c.1));
    out
}

/// Nearest palette entries (opaque, RGB distance) found quickly: the colour cube is cut into
/// 16 x 16 x 16 cells, and each cell keeps the entries that can be nearest to some colour in it
/// (those whose distance to the cell's box is at most the least farthest distance of any
/// entry). The same answers as [`nearest`], ties included (candidates stay in palette order).
struct NearestGrid<'a> {
    palette: &'a [Entry],
    cells: Vec<Option<Vec<u8>>>,
}

impl<'a> NearestGrid<'a> {
    fn new(palette: &'a [Entry]) -> Self {
        NearestGrid {
            palette,
            cells: vec![None; 4096],
        }
    }

    fn nearest(&mut self, c: Entry) -> usize {
        let cell = (usize::from(c[0] >> 4) << 8) | (usize::from(c[1] >> 4) << 4) | usize::from(c[2] >> 4);
        let palette = self.palette;
        let Some(slot) = self.cells.get_mut(cell) else {
            return nearest(palette, c, false);
        };
        let candidates = slot.get_or_insert_with(|| {
            let lo = [c[0] & 0xF0, c[1] & 0xF0, c[2] & 0xF0];
            let span = |p: &Entry, far: bool| -> u32 {
                (0..3)
                    .map(|k| {
                        let (v, l) = (i32::from(p.get(k).copied().unwrap_or(0)), i32::from(lo.get(k).copied().unwrap_or(0)));
                        let h = l + 15;
                        let d = if far {
                            (v - l).abs().max((v - h).abs())
                        } else if v < l {
                            l - v
                        } else if v > h {
                            v - h
                        } else {
                            0
                        };
                        (d * d) as u32
                    })
                    .sum()
            };
            let limit = palette.iter().map(|p| span(p, true)).min().unwrap_or(u32::MAX);
            palette
                .iter()
                .enumerate()
                .filter(|(_, p)| span(p, false) <= limit)
                .map(|(i, _)| i.min(255) as u8)
                .collect()
        });
        let mut best = (u32::MAX, 0usize);
        for &i in candidates.iter() {
            let Some(p) = palette.get(usize::from(i)) else { continue };
            let d: u32 = p
                .iter()
                .zip(&c)
                .take(3)
                .map(|(&a, &b)| {
                    let v = i32::from(a) - i32::from(b);
                    (v * v) as u32
                })
                .sum();
            if d < best.0 {
                best = (d, usize::from(i));
                if d == 0 {
                    break;
                }
            }
        }
        best.1
    }
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
    skip: &(dyn Fn(Entry) -> bool + Sync),
    skip_index: u8,
) -> Vec<u8> {
    let mut cache: HashMap<u32, u8> = HashMap::default();
    // Opaque colours against a large palette: the cell search (same answers, far fewer
    // comparisons; a palette of 256 entries indexes in a byte).
    let mut grid = (!alpha && palette.len() > 16 && palette.len() <= 256).then(|| NearestGrid::new(palette));
    let mut lookup = |c: Entry| -> u8 {
        *cache.entry(key(c)).or_insert_with(|| match grid.as_mut() {
            Some(g) => g.nearest(c).min(255) as u8,
            None => nearest(palette, c, alpha).min(255) as u8,
        })
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
    fn the_cell_search_finds_the_same_entries() {
        // Every 3rd level of the cube against the Windows palette and an adaptive one, ties
        // included (the Web 216 palette has many).
        let mut px = Vec::new();
        for r in (0..=255u8).step_by(7) {
            for g in (0..=255u8).step_by(5) {
                for b in (0..=255u8).step_by(9) {
                    px.push([r, g, b, 255]);
                }
            }
        }
        let hist = histogram(&px);
        for palette in [windows(), web216(), adaptive(&hist, 64, false), uniform(64)] {
            let mut grid = NearestGrid::new(&palette);
            for &c in &px {
                assert_eq!(grid.nearest(c), nearest(&palette, c, false), "{c:?}");
            }
        }
    }

    #[test]
    fn the_best_subset_keeps_a_rare_color_nothing_else_is_near() {
        // Many shades of red and a few pure blue pixels: the most used entries are all reds
        // and blue maps to a red; the best subset spends one entry on blue.
        let palette = web216();
        let mut px = Vec::new();
        for (i, r) in [255u8, 204, 153, 102].into_iter().enumerate() {
            px.extend(std::iter::repeat_n([r, 0, 0, 255], 100 - i * 10));
            px.extend(std::iter::repeat_n([r, 51, 0, 255], 60 - i * 10));
        }
        px.extend(std::iter::repeat_n([0, 0, 255, 255], 5));
        let hist = histogram(&px);
        let error = |pal: &[Entry]| -> u64 {
            px.iter()
                .map(|q| {
                    pal.iter()
                        .map(|c| (0..3).map(|k| (i64::from(q[k]) - i64::from(c[k])).pow(2) as u64).sum::<u64>())
                        .min()
                        .unwrap_or(u64::MAX)
                })
                .sum()
        };
        let most = most_used(&palette, &hist, 8);
        let best = best_subset(&palette, &hist, 8);
        assert_eq!(best.len(), 8);
        assert!(best.iter().all(|c| palette.contains(c)));
        assert!(best.contains(&[0, 0, 255, 255]) && !most.contains(&[0, 0, 255, 255]));
        assert!(error(&best) < error(&most));
        // Enough room: the whole palette.
        assert_eq!(best_subset(&palette, &hist, 256).len(), 216);
    }

    #[test]
    fn a_fitted_grid_spans_the_colors_and_fills_the_count() {
        // Reds 100..200, greens 0..255, blue fixed at 40.
        let mut px = Vec::new();
        for r in (100..=200u8).step_by(5) {
            for g in (0..=255u8).step_by(15) {
                px.push([r, g, 40, 255]);
            }
        }
        let hist = histogram(&px);
        let p = uniform_fitted(&hist, 256);
        // Blue has no spread: one level; red and green share the rest, their steps balanced
        // (10 x 25 = 250 entries: about 11 levels apart in both), where the fixed grid gives the
        // 216 cube.
        assert!(p.len() > 216 && p.len() <= 256, "{}", p.len());
        assert!(p.iter().all(|c| c[2] == 40));
        let reds: std::collections::BTreeSet<u8> = p.iter().map(|c| c[0]).collect();
        let greens: std::collections::BTreeSet<u8> = p.iter().map(|c| c[1]).collect();
        // Within the range, half a slice in from each end.
        let (r0, r1) = (reds.first().copied().unwrap_or(0), reds.last().copied().unwrap_or(0));
        assert!((101..=110).contains(&r0) && (190..200).contains(&r1), "{r0} {r1}");
        let (g0, g1) = (greens.first().copied().unwrap_or(0), greens.last().copied().unwrap_or(0));
        assert!(g0 <= 10 && g1 >= 245, "{g0} {g1}");
        // The wider channel gets at least as many levels.
        assert!(greens.len() >= reds.len());
        // Small counts: never over.
        for n in [2, 3, 8, 16, 100] {
            assert!(uniform_fitted(&hist, n).len() <= n, "{n}");
        }
        // Fits better than the fixed grid.
        let error = |pal: &[Entry]| -> f64 {
            px.iter()
                .map(|q| {
                    pal.iter()
                        .map(|c| (0..3).map(|k| (f64::from(q[k]) - f64::from(c[k])).powi(2)).sum::<f64>())
                        .fold(f64::INFINITY, f64::min)
                })
                .sum()
        };
        assert!(error(&p) < error(&uniform(256)) / 4.0);
    }

    #[test]
    fn best_fit_grays_beat_a_median_cut() {
        // Uneven greys: a dense middle, a thin dark ramp, some white.
        let mut px = Vec::new();
        for v in 100..=140u8 {
            px.extend(std::iter::repeat_n([v, v, v, 255], 50));
        }
        for v in (0..60u8).step_by(3) {
            px.extend(std::iter::repeat_n([v, v, v, 255], 4));
        }
        px.extend(std::iter::repeat_n([255, 255, 255, 255], 30));
        let hist = histogram(&px);
        let error = |pal: &[Entry]| -> f64 {
            px.iter()
                .map(|p| {
                    pal.iter()
                        .map(|q| (f64::from(p[0]) - f64::from(q[0])).powi(2))
                        .fold(f64::INFINITY, f64::min)
                })
                .sum()
        };
        for n in [2, 4, 8, 16] {
            let best = grays_best_fit(&hist, n);
            assert!(best.len() <= n && best.iter().all(|c| c[0] == c[1] && c[1] == c[2]));
            assert!(error(&best) <= error(&adaptive(&hist, n, false)), "{n} greys");
        }
        // Few enough levels: each its own entry, exactly.
        let levels = hist.len();
        let exact = grays_best_fit(&hist, 256);
        assert_eq!(exact.len(), levels);
        assert_eq!(error(&exact), 0.0);
    }

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
    fn adaptive_palettes_are_the_same_every_time() {
        let px: Vec<Entry> = (0..4000u32)
            .map(|i| [(i * 7 % 256) as u8, (i * 13 % 256) as u8, (i * 29 % 256) as u8, 255])
            .collect();
        let first = adaptive(&histogram(&px), 16, false);
        for _ in 0..5 {
            assert_eq!(adaptive(&histogram(&px), 16, false), first);
        }
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
