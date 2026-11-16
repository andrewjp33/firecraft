# Bundled plug-ins (Eye Candy 4000 LE, Splat LE)

Fireworks MX ships Alien Skin's Eye Candy 4000 LE (Bevel Boss, Marble, Motion Trail) and Splat
LE (Edges) as Photoshop `.8bf` plug-ins. These are the **LE ("light") demo editions** bundled
with a new install: a few filters each, with the options listed below. The full commercial Eye
Candy 4000 and Splat products have many more filters and settings; FireCraft covers only what
the bundled LE editions offer (and what documents made with them contain). FireCraft has built-in versions
(`crates/raster/src/plugins.rs`), written clean-room from the plug-ins' published parameters and
from renders; documents keep the original parameters, so they still open with the real
plug-ins in Fireworks.

## Storage

An effect's `EffectMoaID` is `{d810e821-f86b-11d0-XXXXXXXXXXXXXXXX}` where the last 8 bytes are
the plug-in's dictionary tag with each 4-byte half reversed: Bevel Boss `4AceNide`, Motion Trail
`4AceTmde`, Marble `4AceAmde`, Edges `1GqsDegs`. Parameters are a Photoshop scripting
descriptor flattened into `external_filter_key__<code>` keys (`0x<hex>` when the 4-character
code isn't alphanumeric, e.g. `0x466c2020` = `'Fl  '`): numbers as decimal text, booleans as
`true`/`false`, TEXT values as hex ("79.91 pixels", "RGB: 100.0% 0.0% 0.0% (255 0 0)",
"Bevel: 3/x y pointy/..."), enumerations as two hex words (type, value).

Parameter names and option values come from the plug-ins' scripting dictionaries:

| Plug-in | Parameters |
|---|---|
| Bevel Boss | Bevel Width `Wdth`, Bevel Height Scale `Cntr`, Smoothness `Smth`, Bevel Placement `bvlS` (`beT0` inside / `beT1` outside marquee), Shade Interior `Fl  `, Direction `Drct`, Inclination `INcl`, Highlight Brightness `Brgh`, Highlight Size `Shrp`, Highlight Color `Clr `, Shadow Color `sdwC`, Bevel Profile `BEvl`, Darken Deep Areas `Drkn` |
| Motion Trail | Direction `Drct`, Length `Lngt`, Taper `TApe`, Overall Opacity `Opct`, Smear Color from Edges `Edg `, Draw Only Outside Selection `Fl  ` |
| Marble | Bedrock Color `BEco`, Vein Color `VEcl`, Vein Size `FEsi`, Vein Coverage `VEcv`, Vein Thickness `VEth`, Vein Roughness `VEro`, Random Seed `RndS`, Seamless Tile `TIle` |
| Edges | Edge Width `Wdth`, Margin `BrdT`, Feature Size `Scl `, Direction `Drct`, Edge Mode `Edg ` (`edM0` Lumpy, `edM1` Rough, `edM3` Halftone Dots, `edM4` Pixels, `edM5` Halftone Lines, `edM6` Torn Paper), Color `Clr `, Transparent Fill `Fl  `, Random Seed `RndS` |

## Oracle

Under Wine the plug-ins need `MFC42.DLL` (`winetricks mfc42`); scripted re-renders were still
unreliable there, so references come from Fireworks MX on Windows: `tools/fwmx-oracle/
plugin_test.jsf` builds a grid of shapes with one setting varied per cell (its rounded
rectangles use roundness 8, a script bug that makes self-intersecting shapes; use the circles).

## Bevel Boss (fitted, 2026-11-16)

References: the grid's circle cells, then `crates/io/examples/bevel_probe.rs` rendered by
Fireworks MX on Windows (`probes/bevel_probe_win.png`, made by the user): a hard disc, a disc
with a soft halo, a soft alpha ramp with a soft hole, rings whose transparent pixels store white
or black, a square with a hole; × base, Smoothness 0/100, Width 5/30, outside.

- The object is its pixels at least half opaque. The profile's x runs from 0 at `width` inside
  it to 1 at its edge. Profile (`BEvl`, `Bevel: n/x y f/...`): f is Eye Candy's per-point corner
  flag, 1 sharp, 0 smooth. The curve is split at sharp points and each piece is the polynomial
  through its points (two: a straight line; three: the parabola), clamped to 0..1. Measured
  2026-11-16 on `crates/io/examples/rect_bevel_probe.rs` under Wine (tilt from the top edge's
  shading: within 0.5-1.2 degrees for 3-, 4- and 5-point smooth profiles, where Catmull-Rom and
  natural splines were 2-21 degrees off; a sharp middle point gives two straight pieces),
  eye2.png (the parabola) and the user's eye3_groove.png (7 points with sharp corners; its
  centre stays flat where the parabola would dip below 0).
- Height: the profile's drop over the bevel is width x (0.3 + Height Scale/100) (rect probe,
  straight profile: slopes 0.40, 0.55, 0.80 at Height 10, 25, 50; also the cones at Height 28,
  eye3 at 50, eye2 at 76), so Height 0 still bevels. Smoothness blurs it with sigma 5.25 x
  Smoothness/100 px, the same at every width.
- Distance to the edge (inside): measured to the anti-aliased edge line at the nearest outside
  pixel (normal from the Sobel gradient of alpha, offset from its coverage by Gustavson's EDTAA
  formula, perpendicular distance), where alpha has a real edge (gradient at least 1); soft ramps
  keep the plain distance. Distances to pixel centres stepped by up to 0.32 px around a 400 px
  circle and drew radial streaks on large domes (eye2.png); now within 0.07
  (`crates/io/examples/smooth_probe.rs`, eye2's bevel at Smoothness 0-100, under Wine: Fireworks
  MX shows faint streaks at Smoothness 0 and none from about 20).
- Flat areas are lit too: colour x sin(inclination) (196 -> 139 at 45 degrees); colour =
  mix(Shadow Color, object, n.L). Inclination is the light's elevation.
- Highlight: cos(angle(n, H))^e x Brightness, e = 13 + 78 (1 - Size)^0.56, H halfway between
  the light and the viewer. Fitted on `crates/io/examples/bevel_probe2.rs` (cones of constant
  slope, so every angle shows around them; colour without green, so green is the highlight) under
  Wine: exponents 85, 79, 66, 49, 13 at Highlight Size 10, 25, 50, 75, 100 (Size barely matters
  up to 50, then widens fast; eye3.png at Size 100 showed the earlier cone, Size% of 85.5 degrees,
  washing the whole dome out).
- **Inside**, every visible pixel is shaded, also the faint ones beyond the half-opaque edge,
  and keeps its alpha. A semi-transparent pixel's colour is scaled by a / (a + 0.2 (1 - a))
  before shading (measured along the light's perpendicular on `soft_bevel_probe`: 0.42 at alpha
  33, 0.80 at 112, 0.90 at 160, 0.97 at 224; the earlier alpha^0.4 was too dark in the middle).
  spectrum_wave.png (black, alpha 17..205 after Convert to Alpha) bevels along its alpha-128
  contour this way.
- **Outside**, the rim isn't the object's colour: the Shadow Color at alpha 1 - lit with the
  Highlight Color at alpha k over it, painted over the transparent pixels (black and white
  overlays). Transparent pixels' stored colour never matters (the white and black rings render
  identically).
- Colour math is f64 throughout, rounded once, in both colour-math modes (this is a fit, not a
  bit-exact port).
- Results (2026-11-16, after the profile, height and highlight measurements): eye.png 99.2%,
  eye2.png 98.5%, eye3.png 92.3%, eye3_groove.png 98.5%, ball.png 100%, plugin_test_windows.png
  91.4%, spectrum_wave.png 97.6%. Still off: eye3's very wide cone streaks a little more than
  Fireworks MX's, and spectrum_wave's highlight arc is broken where Fireworks MX's is smooth
  (alpha noise against the half-opaque edge; see below).

### Soft alpha and noise (open, 2026-11-16)

`crates/io/examples/soft_bevel_probe.rs` (radial alpha ramps of slope 2/6/20 per pixel, noise
±5/±20, a flat noisy disc, a black ramp; spectrum_wave's settings and variants), rendered on
Windows (`probes/soft_bevel_probe_win.png`), and the spectrum_wave{0,10,...,100}.png series
(only the plaster texture amount differs) show:
- Noise-free soft ramps now match well (crescent error about 7 levels).
- With noise, Fireworks MX keeps the highlight in place and continuous but grainy (±20 alpha
  noise on a 2-per-pixel ramp; spectrum_wave stays smooth to 10-20% texture). FireCraft's hard
  half-opaque cut wobbles with the noise and breaks the highlight. Smoothing alpha before the cut
  makes it continuous but thin and dim; MX's stays broad and grainy, so MX lets the noise into
  the shading while locating the edge from smoothed alpha. Not solved.
- At a hard edge of a soft object (alpha 180 inside, 0 outside) MX smooths the height without
  renormalising (outside pixels count at the edge's height), which keeps a strong highlight;
  FireCraft's normalised smoothing flattens it. Switching improves both probes and
  plugin_test_windows.png (87.2 -> 88.4%) but costs ball.png 3% (99.5 -> 96.5), unexplained,
  so not adopted.
- A distance weighted by how far alpha is past 50% (ramping over 10 levels) helps slightly
  (bevel probe 140 -> 133 with plain smoothing); not adopted on its own.

Edges and Marble are first versions from the reference files (sun) and the test grid, not yet
fitted.

## Motion Trail (fitted, 2026-11-16)

References: `crates/io/examples/trail_probe.rs` (a 60 px ringed disc and a disc with a 12 px soft
halo, one setting per cell) rendered by Fireworks MX on Windows (`probes/trail_probe_win.png`
next to the repo, made by the user), and SOLAR.png. Fireworks MX also runs Eye Candy under Wine
once `mfc42.dll` is in the prefix's `syswow64`; shape and alpha come out right there but blue is
lost (about value/20), also when the effects are re-applied from a script.

- The object's shape is its pixels at least half opaque. Copies of it are shifted k = 1..Length
  pixels along the direction, each squeezed across the direction by 1 - Taper·k/57.3 about the
  object's centre line. The narrowing doesn't depend on Length or on the object's size (fits
  55-60 px on the disc and on SOLAR's 119 px and 231 px planets), so long trails taper to a point.
- A trail pixel takes the nearest copy covering it: alpha Opacity × (1 - k/Length), linear over
  exactly Length whatever the Taper. A soft-edged object therefore trails at full strength from
  its half-opaque outline. The trail is then softened (Gaussian, sigma 1) and drawn behind the
  object.
- Colour (Smear Color from Edges): the object's pixel the copy maps to, averaged across the
  direction (Gaussian, sigma 2; fitted on SOLAR, the probe is indifferent).
- Smear Color from Edges off: not matched. Fireworks MX then draws the trail over the object
  too, with colours dragged across from the object's far side (the halo disc's red sector runs
  over yellow and green).
- Precise colour math: the trail stays in f64 through the softening and the compositing and is
  rounded once; the softening weights colour by alpha (Fireworks MX's own Gaussian, used in
  Classic, averages the colours of all pixels with any alpha alike). Same accuracy overall against
  the Windows render; at sharp colour borders on the trail's edge it is closer (411 of 575 pixels
  that differ by more than 4 levels between the modes).
- Accuracy: trail alpha on the probe within about 5 levels on average for the plain disc and 9-13
  for the halo disc (the halo's outer sides trail a little further in Fireworks MX); SOLAR.png 99.0%.
- SOLAR.png on Windows sometimes shows a green cast on first render; opening the effect and
  pressing Cancel re-renders it correctly (the user's observation). Check a reference for that
  before fitting colours to it.
