//! Compare FireCraft's rendering of Fireworks PNGs with the flattened image Fireworks itself
//! stored in each file (its `IDAT`), and check lossless/save round trips.
//!
//! Usage: parity_report <out-dir> <file.png>...
//! Writes into <out-dir>: <name>.cmp.png (Fireworks | FireCraft | difference) for each file,
//! report.txt (the table it prints), results.tsv, and index.html (the table with the
//! comparisons, and each file's change since the previous run).
//!
//! Environment: PARITY_LABEL (a title for the run, e.g. date and commit), PARITY_PREV (the
//! previous run's folder, for the changes), PARITY_KEEP (when <out-dir> is one of several run
//! folders in a parent: how many of the newest runs keep their images, default 10; older runs
//! keep only their tables). The parent folder gets an index.html listing all runs.
//!
//! Frame references: a plain exported PNG named `<doc>_frame<N>.png` next to `<doc>.png` is
//! compared with FireCraft's render of frame N of `<doc>.png` (export single frames from
//! Fireworks as PNG32 to test animations).
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().ok_or("out dir")?);
    std::fs::create_dir_all(&out)?;
    let header = format!(
        "{:<20} {:>9} {:>6} {:>7} {:>7} {:>6} {:>6}  notes",
        "file", "size", "raw-rt", "match%", "meanΔ", "open", "save"
    );
    println!("{header}");
    let mut report = vec![header];
    let mut rows: Vec<Row> = Vec::new();
    for path in args {
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = std::fs::read(&path)?;
        let raw_rt = firecraft_fwpng::FwPng::read(&bytes)
            .map(|p| p.to_bytes() == bytes)
            .unwrap_or(false);
        let t = std::time::Instant::now();
        let doc = match firecraft_io::open(&bytes, Some(path.clone())) {
            Ok(d) => d,
            Err(e) => {
                let line = format!("{name:<20} OPEN FAILED: {e}");
                println!("{line}");
                report.push(line);
                rows.push(Row {
                    name,
                    size: String::new(),
                    raw_rt,
                    matched: 0.0,
                    mean: 0.0,
                    save_ok: false,
                    plain: false,
                    failed: true,
                    notes: format!("open failed: {e}"),
                });
                continue;
            }
        };
        let open_ms = t.elapsed().as_millis();
        let fw = firecraft_io::decode_png(&bytes)?;
        // A frame export: render that frame of the source document instead.
        let frame_ref = (doc.raw.is_none()).then(|| frame_source(&path)).flatten();
        let (doc, frame) = match frame_ref.clone() {
            Some((src, f)) => match std::fs::read(&src)
                .ok()
                .and_then(|b| firecraft_io::open(&b, Some(src.clone())).ok())
            {
                Some(d) => (d, f),
                None => (doc, 0),
            },
            None => (doc, 0),
        };
        let ours = doc.render(frame);
        let (w, h) = (fw.width().min(ours.width()), fw.height().min(ours.height()));
        let (mut ok, mut sum) = (0u64, 0u64);
        let mut diff = RgbaImage::new(w, h, Rgba::BLACK);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (a, b) = (
                    fw.get(x, y).unwrap_or_default(),
                    ours.get(x, y).unwrap_or_default(),
                );
                // Compare as composited over white, so fully transparent pixels compare equal.
                let wa = Rgba::WHITE.over(a, 255);
                let wb = Rgba::WHITE.over(b, 255);
                let d = [wa.r.abs_diff(wb.r), wa.g.abs_diff(wb.g), wa.b.abs_diff(wb.b)]
                    .into_iter()
                    .max()
                    .unwrap_or(0);
                sum += d as u64;
                if d <= 8 {
                    ok += 1;
                }
                let v = (d as u32 * 4).min(255) as u8;
                diff.set(x, y, Rgba::new(v, v, v, 255));
            }
        }
        let n = (w as u64 * h as u64).max(1);
        // Save round trip: FireCraft's own save must re-open to the same render.
        let save_ok = firecraft_io::save_fireworks_png(&doc)
            .ok()
            .and_then(|b| firecraft_io::open(&b, None).ok())
            .map(|d2| d2.render(frame) == ours)
            .unwrap_or(false);
        let mut notes = String::new();
        // A plain PNG (an export, no Fireworks layer data) opens as one bitmap of its own
        // image: comparing it with itself proves nothing.
        if frame_ref.is_some() {
            notes += &format!("frame {} of its document; ", frame + 1);
        } else if doc.raw.is_none() {
            notes += "PLAIN PNG (no Fireworks data; not a parity test); ";
        }
        if (fw.width(), fw.height()) != (ours.width(), ours.height()) {
            notes += &format!(
                "size fw {}x{} vs ours {}x{}; ",
                fw.width(),
                fw.height(),
                ours.width(),
                ours.height()
            );
        }
        let kinds = doc
            .layers
            .iter()
            .flat_map(|l| l.elements(0))
            .map(|e| e.label())
            .collect::<Vec<_>>();
        notes += &format!(
            "{} layers, {} top-level objects, {} frames",
            doc.layers.len(),
            kinds.len(),
            doc.frames.len()
        );
        let size = format!("{}x{}", doc.width, doc.height);
        let (matched, mean) = (ok as f64 * 100.0 / n as f64, sum as f64 / n as f64);
        let line = format!(
            "{name:<20} {size:>9} {:>6} {matched:>6.1}% {mean:>7.2} {:>4}ms {:>6}  {notes}",
            if raw_rt { "ok" } else { "DIFF" },
            open_ms,
            if save_ok { "ok" } else { "DIFF" },
        );
        println!("{line}");
        report.push(line);
        rows.push(Row {
            name: name.clone(),
            size,
            raw_rt,
            matched,
            mean,
            save_ok,
            plain: frame_ref.is_none() && doc.raw.is_none(),
            failed: false,
            notes,
        });
        // Side-by-side image, downscaled for big documents.
        let scale = ((1200.0 / w.max(1) as f64).min(600.0 / h.max(1) as f64)).min(1.0);
        let (sw, sh) = (
            ((w as f64 * scale) as u32).max(1),
            ((h as f64 * scale) as u32).max(1),
        );
        let mut sheet = RgbaImage::new(sw * 3 + 20, sh, Rgba::new(128, 128, 128, 255));
        for (i, img) in [&fw, &ours, &diff].into_iter().enumerate() {
            for y in 0..sh {
                for x in 0..sw {
                    let p = img
                        .get((x as f64 / scale) as i64, (y as f64 / scale) as i64)
                        .unwrap_or_default();
                    sheet.set(
                        (i as u32 * (sw + 10) + x) as i64,
                        y as i64,
                        Rgba::WHITE.over(p, 255),
                    );
                }
            }
        }
        std::fs::write(
            out.join(format!("{name}.cmp.png")),
            firecraft_io::encode_png(&sheet, 72.0)?,
        )?;
    }
    std::fs::write(out.join("report.txt"), report.join("\n") + "\n")?;
    let tsv: String = rows
        .iter()
        .map(|r| {
            format!(
                "{}\t{:.2}\t{:.3}\t{}\t{}\t{}\t{}\t{}\n",
                r.name, r.matched, r.mean, r.raw_rt, r.save_ok, r.plain, r.failed, r.notes
            )
        })
        .collect();
    std::fs::write(out.join("results.tsv"), tsv)?;
    let label = std::env::var("PARITY_LABEL").unwrap_or_else(|_| out.display().to_string());
    let prev_dir = std::env::var("PARITY_PREV").ok().filter(|p| !p.is_empty());
    let prev = prev_dir
        .as_ref()
        .map(|d| read_results(&std::path::Path::new(d).join("results.tsv")))
        .unwrap_or_default();
    let prev_name = prev_dir.as_ref().and_then(|d| {
        std::path::Path::new(d.trim_end_matches('/'))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    });
    std::fs::write(
        out.join("index.html"),
        run_page(&label, &rows, &prev, prev_name.as_deref()),
    )?;
    if let Some(parent) = out.parent() {
        let keep = std::env::var("PARITY_KEEP")
            .ok()
            .and_then(|k| k.parse().ok())
            .unwrap_or(10usize);
        runs_index(parent, keep)?;
    }
    println!("Comparisons and summary: {}", out.join("index.html").display());
    Ok(())
}

struct Row {
    name: String,
    size: String,
    raw_rt: bool,
    matched: f64,
    mean: f64,
    save_ok: bool,
    /// A plain PNG (no Fireworks data): listed, but not a parity test.
    plain: bool,
    failed: bool,
    notes: String,
}

/// A run's results.tsv: file name -> (match %, counts as a parity test).
fn read_results(path: &std::path::Path) -> std::collections::HashMap<String, (f64, bool)> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let matched = f.get(1)?.parse().ok()?;
            let test = f.get(5) == Some(&"false") && f.get(6) == Some(&"false");
            Some(((*f.first()?).to_owned(), (matched, test)))
        })
        .collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const STYLE: &str = "body{font:14px system-ui,sans-serif;margin:16px;background:#f4f4f4;color:#222}
h1{font-size:20px;margin:0 0 4px}.sub{color:#666;margin-bottom:12px}
table{border-collapse:collapse;background:#fff}th,td{padding:4px 8px;border-bottom:1px solid #ddd;text-align:left;vertical-align:top}
th{background:#e8e8e8;cursor:pointer;position:sticky;top:0}td.n{text-align:right;font-variant-numeric:tabular-nums}
.up{color:#0a7d2c;font-weight:600}.down{color:#c0262d;font-weight:600}.same{color:#999}.bad{color:#c0262d;font-weight:600}
.plain{color:#999}img{max-width:420px;border:1px solid #ccc;background:#fff}.notes{color:#666;font-size:12px;max-width:260px}
.bar{display:inline-block;height:8px;background:#3b82f6;vertical-align:middle;margin-left:6px}";

/// Sorting the table by a clicked column.
const SORT: &str = "<script>document.querySelectorAll('th').forEach((th,i)=>th.onclick=()=>{
const tb=th.closest('table').tBodies[0],rows=[...tb.rows],asc=th.dataset.asc!=='1';th.dataset.asc=asc?'1':'0';
const v=r=>{const c=r.cells[i];const d=c.dataset.v;return d!==undefined?parseFloat(d):c.textContent};
rows.sort((a,b)=>{const x=v(a),y=v(b);return (x>y?1:x<y?-1:0)*(asc?1:-1)});rows.forEach(r=>tb.appendChild(r))})</script>";

fn change(now: f64, before: Option<f64>) -> String {
    match before {
        Some(b) if (now - b).abs() >= 0.05 => {
            let class = if now > b { "up" } else { "down" };
            format!(
                "<td class=\"n {class}\" data-v=\"{:.2}\">{:+.1}</td>",
                now - b,
                now - b
            )
        }
        Some(_) => "<td class=\"n same\" data-v=\"0\">=</td>".into(),
        None => "<td class=\"n same\" data-v=\"0\">new</td>".into(),
    }
}

/// One run's page: summary, then each file with its comparison image.
fn run_page(
    label: &str,
    rows: &[Row],
    prev: &std::collections::HashMap<String, (f64, bool)>,
    prev_name: Option<&str>,
) -> String {
    let tests: Vec<&Row> = rows.iter().filter(|r| !r.plain && !r.failed).collect();
    let avg = tests.iter().map(|r| r.matched).sum::<f64>() / tests.len().max(1) as f64;
    let (mut up, mut down) = (0, 0);
    for r in &tests {
        if let Some((b, _)) = prev.get(&r.name) {
            if r.matched > b + 0.05 {
                up += 1;
            } else if r.matched < b - 0.05 {
                down += 1;
            }
        }
    }
    let lossless = rows.iter().filter(|r| r.raw_rt && r.save_ok).count();
    let mut h = format!(
        "<!doctype html><meta charset=utf-8><title>Parity {}</title><style>{STYLE}</style>
<h1>FireCraft vs Fireworks: {}</h1><div class=sub>{} files ({} parity tests): average match {avg:.1}%;
{lossless}/{} lossless round trips{}. Each image: Fireworks | FireCraft | difference (brighter = more different).
<a href=\"../index.html\">All runs</a></div>
<table><thead><tr><th>file</th><th>match %</th><th>change</th><th>mean Δ</th><th>size</th><th>round trips</th><th>notes</th><th>Fireworks | FireCraft | difference</th></tr></thead><tbody>",
        esc(label),
        esc(label),
        rows.len(),
        tests.len(),
        rows.len(),
        match prev_name {
            Some(p) => format!("; since <a href=\"../{0}/index.html\">{0}</a>: {up} better, {down} worse", esc(p)),
            None => String::new(),
        },
    );
    for r in rows {
        let img = format!("{}.cmp.png", r.name);
        let class = if r.plain { " class=plain" } else { "" };
        let trips = match (r.raw_rt, r.save_ok) {
            (true, true) => "ok".to_owned(),
            (raw, save) => format!(
                "<span class=bad>{}{}</span>",
                if raw { "" } else { "file DIFF " },
                if save { "" } else { "save DIFF" }
            ),
        };
        h += &format!(
            "<tr{class}><td>{name}</td><td class=n data-v=\"{m:.2}\">{m:.1}<span class=bar style=\"width:{w:.0}px\"></span></td>{chg}<td class=n data-v=\"{mean:.3}\">{mean:.2}</td><td>{size}</td><td>{trips}</td><td class=notes>{notes}</td><td><a href=\"{img}\"><img loading=lazy src=\"{img}\" alt=\"\"></a></td></tr>\n",
            name = esc(&r.name),
            m = r.matched,
            w = r.matched * 0.6,
            chg = if r.plain || r.failed {
                "<td></td>".into()
            } else {
                change(r.matched, prev.get(&r.name).map(|p| p.0))
            },
            mean = r.mean,
            size = esc(&r.size),
            notes = esc(&r.notes),
            img = esc(&img),
        );
    }
    h + "</tbody></table>" + SORT
}

/// The parent folder's index.html: every run (newest first) with its average match; runs
/// beyond the newest `keep` lose their comparison images (their tables stay).
fn runs_index(parent: &std::path::Path, keep: usize) -> std::io::Result<()> {
    let mut runs: Vec<String> = std::fs::read_dir(parent)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("results.tsv").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    runs.sort();
    runs.reverse();
    for old in runs.iter().skip(keep) {
        for e in std::fs::read_dir(parent.join(old))?.filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().ends_with(".cmp.png") {
                std::fs::remove_file(e.path())?;
            }
        }
    }
    let mut h = format!(
        "<!doctype html><meta charset=utf-8><title>Parity runs</title><style>{STYLE}</style>
<h1>FireCraft vs Fireworks: parity runs</h1><div class=sub>Newest first. Average match over the files that are parity tests in each run
(the reference set grows over time, so compare a file's own history in its run pages). The newest {keep} runs keep their comparison images.</div>
<table><thead><tr><th>run</th><th>files</th><th>average match %</th><th>change</th></tr></thead><tbody>"
    );
    let stats: Vec<(String, usize, f64)> = runs
        .iter()
        .map(|r| {
            let res = read_results(&parent.join(r).join("results.tsv"));
            let tests: Vec<f64> = res.values().filter(|v| v.1).map(|v| v.0).collect();
            (
                r.clone(),
                res.len(),
                tests.iter().sum::<f64>() / tests.len().max(1) as f64,
            )
        })
        .collect();
    for (i, (run, n, avg)) in stats.iter().enumerate() {
        let before = stats.get(i + 1).map(|s| s.2);
        h += &format!(
            "<tr><td><a href=\"{0}/index.html\">{0}</a></td><td class=n>{n}</td><td class=n data-v=\"{avg:.2}\">{avg:.1}<span class=bar style=\"width:{w:.0}px\"></span></td>{chg}</tr>\n",
            esc(run),
            w = avg * 0.6,
            chg = change(*avg, before),
        );
    }
    std::fs::write(parent.join("index.html"), h + "</tbody></table>" + SORT)
}

/// `dir/<doc>_frame<N>.png` -> (`dir/<doc>.png`, N - 1) when that document exists.
fn frame_source(path: &str) -> Option<(String, usize)> {
    let p = std::path::Path::new(path);
    let stem = p.file_stem()?.to_str()?;
    let (doc, n) = stem.rsplit_once("_frame")?;
    let n: usize = n.parse().ok().filter(|&n| n >= 1)?;
    let src = p.with_file_name(format!("{doc}.png"));
    src.exists().then(|| (src.to_string_lossy().into_owned(), n - 1))
}
