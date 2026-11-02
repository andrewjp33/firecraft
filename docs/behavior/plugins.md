# Bundled plug-ins (Eye Candy 4000 LE, Splat LE)

Fireworks MX ships Alien Skin's Eye Candy 4000 LE (Bevel Boss, Marble, Motion Trail) and Splat
LE (Edges) as Photoshop `.8bf` plug-ins. FireCraft has built-in versions
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

## Bevel Boss (fitted to the circle cells)

- Flat areas are lit too: colour x sin(inclination) (196 -> 139 at 45 degrees); colour =
  mix(Shadow Color, object, n.L). Inclination is the light's elevation.
- The profile's x runs from 0 at `width` inside the object to 1 at its edge (the default
  curve, rising to the edge, makes a dish: the side facing the light is dark).
- Height = profile x width x Height Scale/100 x 1.5; Smoothness blurs it (sigma 0.35 x
  Smoothness/100 x width).
- Highlight: linear cone, 1 - angle(n, H) / (Highlight Size% of 45 degrees), x Brightness.

Still off: heavy Smoothness and large Highlight Size cells, a narrow rim at the very edge, and
colour saturation on eye.png. Motion Trail, Edges and Marble are first versions from the
reference files (SOLAR, sun) and the test grid, not yet fitted.
