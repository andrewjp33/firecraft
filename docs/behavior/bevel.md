# Bevel and Emboss (Fireworks MX)

`EffectMoaID` `{7fe61102-6ce2-11d1-8c76000502701850}`, `BevelType` 0 Inner Bevel, 1 Outer Bevel,
2 Raised Emboss, 3 Inset Emboss (4 is the Glow, see effects.md). Implemented in
`crates/raster/src/bevel.rs`.

Sources: disassembly of `Fireworks.exe` (edge-shape function `0x5dcad0`, lighting `0x5dcbc0`,
integer atan2 `0x5649f0` with its table at `0xe83f64`/`0xe83fa4`, distance-vector passes
`0x5d9260`/`0x5d9d10`/`0x5d9f90`, driver `0x5de1a0`), then fitted and checked against Fireworks
renders of 60x60 squares (`tools/fwmx-oracle`, grids of every type, edge shape, button state,
light angle, colour and softness).

## Algorithm

1. **Band.** Inner: object pixels (alpha >= 128) at Euclidean distance `d` <= width from the
   nearest outside pixel (`d` = 1 for the edge pixel). Outer/Emboss: outside pixels within
   width of the object, anti-aliased: coverage = clamp(width + 1 - d, 0, 1).
2. **Normal.** The outward direction to/from the nearest edge pixel, as Fireworks' integer
   atan2 in degrees (y down; rounded via a 16-segment table). Where two edges are equally near
   (mitred corners) and within 90 degrees of each other (|sum| >= sqrt(2) d), the sum of both
   vectors is used.
3. **Edge shape** `s` at `t = d / width` (Outer: `1 - d / width`):

   | SlopeType | Name | profile(t) |
   |---|---|---|
   | 0 | Flat | 1 |
   | 1 | Smooth | 1 - t |
   | 2 | Sloped | t |
   | 3 | Frame 1 | 1 if t <= 0.65, else -1 |
   | 4 | Frame 2 | 1 if t <= 0.4; 0 if t <= 0.7; else -1 |
   | 5 | Ring | cos(pi t) |
   | 6 | Ruffle | cos(3 pi t), negated for 1/3 < t < 2/3 |

   `s = profile(t) * SlopeMultiplier`; if |s| > 1 then `s = 1/s` (so a multiplier of 2 halves a
   Flat bevel). Embosses ignore the edge shape (`s = 1`). Inset Emboss and button states Down /
   Over While Down negate `s`.
4. **Lighting.** `a = LightAngle + normal` (minus 180 if s < 0), wrapped to (-180, 180]:
   - highlight = round(255 * max(0, 1 - |a|/90))
   - shadow = round(255 * max(0, |a|/120 - 1/2)); Embosses: round(255 * max(0, |a|/90 - 1))
   - each multiplied by |s| * BevelContrast/100 and truncated. Highlight and shadow overlap for
     60 < |a| < 90.
5. **Compositing.** Inner: ShadowColor then HiliteColor over the object's band pixels (object
   alpha kept). Outer: a band of OuterBevelColor under the object, then shaded. Emboss: shading
   over a transparent band; the object shows EmbossFaceColor (default transparent) unless
   ShowObject.
6. **Button states** (`ButtonState`): 1 Over = HitBlendColor (`#ffffff3f`) over the result,
   2 Down = DownBlendColor (`#0000003f`), 3 Over While Down = HitBlendColor; 2 and 3 also invert.
7. **AngleSoftness** (capped at 10) smooths the signed tilt field `s * normal` with a Gaussian of
   sigma ~0.26 x softness: the shading fades near the band's edges and where the shape changes
   sign (Frame transitions). Fitted on straight edges (softness 3 and 10) to +-1 level.

## Accuracy

Softness 0: every tested case is within 2-8 levels of Fireworks (all types, shapes, buttons,
colours, contrast, light angles 0/45/90/100/135).

Known differences:

- Fireworks tilts horizontal/vertical normals by ~1-2 degrees (e.g. the top edge acts like -88.3
  degrees), worth up to ~5 levels on those edges; its source (in the distance-vector passes)
  isn't identified yet.
- With softness > 0, mitred corners differ: Fireworks keeps more strength along the lit diagonal
  and is nearly neutral on the cross-lit one. Matching it needs the smoothing pass `0x5d9f90`
  read exactly. Straight edges match.
- `EdgeThreshold`, `MaskSoftness`, `LightDistance` are not used yet (no visible effect found).
