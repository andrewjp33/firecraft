# Rendering parity

Every Fireworks PNG carries Fireworks' own flattened render (its `IDAT`), so FireCraft can be
scored against Fireworks on any real file without running Fireworks:

```sh
cargo run --release -p firecraft-io --example parity_report -- OUT_DIR files/*.png
```

For each file it checks the lossless container round trip, opens and renders the document,
compares with Fireworks' render (match% = pixels within 8 levels, composited over white), checks
that a FireCraft save re-opens to the same render, and writes `OUT_DIR/<name>.cmp.png`
(Fireworks | FireCraft | difference).

## Reference set (21 real documents, 2007-2026; 20 Fireworks MX, 1 Fireworks CS3)

As of 2026-11-01: container round trip and FireCraft save/re-open are lossless for all 21 files.
Render match ranges from 0% to 87%. The gaps are features FireCraft doesn't render yet. Their use
across the set (objects inside layers):

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
Remaining gaps: bevels, text, decorative gradients, Eye Candy plug-ins.

Fixed thanks to this set: objects within a layer are stored **top first** (`ELMv`), the reverse of
drawing order.
