# Rendering parity

Every Fireworks PNG carries Fireworks' own flattened render (its `IDAT`), so FireCraft can be
scored against Fireworks on any real file without running Fireworks:

```sh
cargo run --release -p firecraft-io --example parity_report -- OUT_DIR files/*.png
make parity            # the same over ../ref_fw_images (REF_DIR=...)
```

`make parity` keeps every run in its own folder, `../parity_runs/<date>_<time>_<commit>/`
(`+changes` when there were uncommitted edits), next to the reference images. Open
`../parity_runs/index.html` to see all runs with their average match and the change from the
previous run; each run's `index.html` lists every file with its match, its change since the
previous run (green better, red worse), round-trip status and the comparison image (click to
enlarge, click a column title to sort). Each run folder also has `report.txt` (the printed
table) and `results.tsv`. The newest 10 runs keep their images (`PARITY_KEEP=N` to change);
older runs keep their tables. `PARITY_RUNS=dir` puts the runs elsewhere. The latest run is also
copied to `target/parity/` (replaced each run; `PARITY_OUT=dir` to change), its links pointing
back to the folder of all runs.

For each file it checks the lossless container round trip, opens and renders the document,
compares with Fireworks' render (match% = pixels within 8 levels, composited over white), checks
that a FireCraft save re-opens to the same render, and writes `OUT_DIR/<name>.cmp.png`
(Fireworks | FireCraft | difference).

## Reference set

The set started as 21 real documents (2007-2026; 20 Fireworks MX, 1 Fireworks CS3) and has grown
to 50 files: Fireworks CS5 test files (9-slice symbols, per-object locks), files saved by
FireCraft for compatibility checks (`*_fc*`), and a few plain PNG screenshots (`r1`-`r6`; the
report marks them "not a parity test").

As of 2026-11-02: container round trip and FireCraft save/re-open are lossless for every file.
Render match on the original documents ranges from 34% (spectrum_wave) to 100%; most are above
80%. The feature use below is for the original 21 files (objects inside layers):

| Feature | Use | Files |
|---|---|---|
| Live effects | Hue/Saturation 105, Gaussian Blur 9, Color Fill 9, Convert to Alpha 9, Bevel 8, Glow 6, Drop Shadow 6, Curves 4, Brightness/Contrast 4, Inner Glow 4, Invert 3, Radial Blur 3, Motion Blur 2, Motion Trail 2, Levels/Auto Levels 2, Edges 1 | 15 |
| Gradient fills | Linear 35, Rectangular 33, Satin/Pinch/Ripple 2 each, Circular/Waves/Bars 1 | many |
| Blend modes (`BLDi`) | value 13: 106, 10/2/5: 1 each | 6 |
| Symbol instances (`ALSv`) | 119 | 4 |
| Animation | `tubes.png`: 103 frames | 1 |

Progress (match %): blend modes took tubes2 12 -> 56; gradient fills took cone to 100;
masks and hidden objects took cone3 0.5 -> 99, comicBorder 62 -> 99.7, 769_edit2 51 -> 100,
hd 37 -> 90, tablet 30 -> 90; live effects (measured Gaussian blur) took circles_hi 0.8 -> 30,
tubes2 56 -> 76, SOLAR 68 -> 76. Symbols took box 43 -> 99.8, tubes 23 -> 77.5, mechscene2 42 -> 65, bolWNWide 0 -> 34.

Current (2026-11-02): 769_edit2 100, box 99.8, comicBorder 99.7, cone3 99, tablet 97.9,
hd 94.6, gym 94.2, tubes 91.1, tubes2 89.0, tm 87.1, circles_hi 82.6, bolWNWide 80.7, sun 80.4,
SOLAR 75.7, eye 74.3, mechscene2 68.0, ball 66.5, spectrum_wave 33.6.
Remaining gaps: text (FireCraft still shows Fireworks' cached text images), bevels with
softness, decorative gradients, stroke rendering (1-px strokes), and parts of the Eye Candy
plug-ins.

Fixed thanks to this set: objects within a layer are stored **top first** (`ELMv`), the reverse of
drawing order.
