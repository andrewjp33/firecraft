# Magic Wand (Fireworks MX behaviour, measured)

All numbers below were measured by driving Fireworks MX (build shipped with Fireworks MX, run under
Wine 9 on Linux) with `tools/fwmx-oracle/wand_edge.sh`: open a test PNG, pick the Magic Wand with
settings written into the preferences file, click, press Delete, save, then read back the alpha
channel. They are encoded as unit tests in `crates/raster/src/wand.rs`.

## Settings

The Property inspector shows **Tolerance** (0-255), **Edge** (Hard Edge / Anti-alias / Feather) and
a number box that is the **feather amount**; it is only enabled when Edge is Feather (grayed and 0
otherwise, by design, also in the shipped help: *Using Fireworks MX > Bitmap selection tool
options*). The preferences file stores `WandToolTolerance`, `WandToolAntiAliased` and
`WandToolFeatherAmount`; the edge mode is derived as *feather > 0 → Feather, else anti-aliased →
Anti-alias, else Hard*. Scripts reach the same code through
`dom.selectSimilarFromPoint(p1, tolerance, edgemode, feather, combineMode)` with edge modes
`"hard edge"`, `"antialias"`, `"feather"` and combine modes `"replace"`, `"add"`, `"subtract"`,
`"intersect"`.

## Measured algorithm

1. **Distance** from the clicked (seed) colour, per pixel:
   `d = max(0, max(ΔR, ΔG, ΔB)) + max(0, −min(ΔR, ΔG, ΔB))`
   (largest channel increase plus largest channel decrease). Verified on 18 colour offsets, e.g.
   Δ = (+30,+30,+30) → 30, (0,+60,−20) → 80, (0,−60,+60) → 120.
2. **Region**: pixels 4-connected to the seed with `d ≤ tolerance`.
3. **Hard**: region selected at 255.
4. **Anti-alias**: additionally, every pixel *outside* the region that is 4-adjacent to it (a
   one-pixel ring; diagonal neighbours are not included) is selected at
   `clamp(255 + tolerance − d, 0, 255)`.
5. **Feather**: anti-alias mask, then feathered by the amount.

Delete then scales each pixel's alpha by `1 − selection/255`; fully removed pixels become
`ARGB 00FFFFFF`.

| Test (100×100 white with a hard red disc, r = 30) | Tolerance | Edge pixels (alpha after Delete) |
|---|---|---|
| Anti-alias | 0 | none: identical to Hard |
| Anti-alias | 10 | 172 px at alpha 245 |
| Anti-alias | 32 | 172 px at alpha 223 |
| Anti-alias | 102 | 172 px at alpha 153 |
| Hard | 102 | none |

On a soft-edged (anti-aliased) disc the ring follows the same formula (e.g. tolerance 32, ring pixel
`(255, 131, 131)` → selected 156).

## Consequence: "Anti-alias does nothing"

On hard-edged artwork (screenshots, pixel art, flat colours) `d` across the edge is large, so the
edge strength is simply the tolerance. Low tolerances therefore give an invisible edge (0 gives
none at all), and the effect only becomes visible around tolerance ~100, where the region starts to
swallow neighbouring colours. This is Fireworks MX's design, not an incompatibility with modern
computers; it matches user reports that the setting is "very picky".

## FireCraft

* `AntiAliasStyle::Classic` reproduces the above exactly.
* `AntiAliasStyle::Smooth` (default) keeps the same region but computes the ring's coverage from the
  image: each ring pixel is *unmixed* between the seed colour and the colour beyond the edge
  (least squares over RGB), which recovers the true partial coverage of anti-aliased artwork, and
  hard staircase edges get geometric smoothing (the share of the pixel's 3×3 block inside the
  region). Tolerance then only decides membership; the edge is visible at every tolerance.

Unknowns: how alpha in the source participates in the distance (all tests used opaque images), and
the region's connectivity for diagonal-only paths (assumed 4-connected like the ring).
