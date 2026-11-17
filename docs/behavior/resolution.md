# Resolution (pixels/inch and pixels/cm)

How Fireworks MX stores, writes, reads and converts a document's resolution, and how FireCraft
matches it. Everything here was measured in Fireworks MX under Wine (2026-11-10 and 2026-11-16)
unless marked otherwise; the New Document unit conversion was also checked in Fireworks CS6.

The aim is that a file or clipboard data from FireCraft opens or pastes in Fireworks with
exactly the resolution and unit it would have had there, with nothing to convert or fix.

## Where the resolution lives

A Fireworks PNG holds the resolution in three places, each with its own precision:

| Where | Holds | Precision |
|---|---|---|
| Tree: `MKBv > RESf`, `RSUi` | the editable resolution, in the document's own unit (`RSUi` 0 = pixels/inch, 1 = pixels/cm) | float32 as text (`71.989998` for 71.99) |
| Flat image: `pHYs` | pixels per metre, x = y, unit 1 (metre) | whole numbers |
| JPEG export: JFIF density | dots per inch | whole numbers |

The tree is the only exact record. Fireworks reads the tree when it opens or pastes a Fireworks
PNG, so `pHYs` only matters to other programs and to flat files Fireworks opens.

## Writing `pHYs` (saves, PNG exports, clipboard data)

Fireworks MX truncates the resolution to a whole number **in its own unit** first, then
converts with integer arithmetic:

* pixels/inch: `trunc(ppi) * 10000 / 254` (integer division)
* pixels/cm: `trunc(px/cm) * 100`

Measured on documents made by script (`fw.createFireworksDocument(size, {pixelsPerUnit,
units: "inch" | "cm"}, color)`) and saved:

| Resolution | `pHYs` (px/m) | Note |
|---|---|---|
| 72 ppi | 2834 | 72 / 0.0254 = 2834.6, truncated |
| 72.5, 72.01 ppi | 2834 | the fraction is dropped before converting |
| 71.99 ppi | 2795 | = 71 ppi |
| 96 ppi | 3779 | |
| 150 ppi | 5905 | |
| 254 ppi | 10000 | a float `ppi * 39.37` would give 9999 |
| 300 ppi | 11811 | |
| 600 ppi | 23622 | |
| 1 ppi | 39 | |
| 2540 ppi | 100000 | |
| 28.35, 28.3465, 28.99 px/cm | 2800 | |
| 37.8 px/cm | 3700 | |
| 72 px/cm | 7200 | |
| 0 px/cm | 0 | (a reference file) |

PNG 8 and PNG 32 exports write the same values, and so does the clipboard PNG.

Of the reference files Fireworks MX saved, 95 have 2834 (72 ppi). FireCraft used to round
(2835 for 72 ppi) and ignored the pixels/cm unit (a 28.35 px/cm document was written as if it
were 28 ppi). It now uses Fireworks' rule: `firecraft_io::png_ppm`, used by `encode_png_at`,
`save_fireworks_png`, `export_png` and the export module's PNG writers.

FirePixel uses the same rule, measured on CS6 (`phys_ppm`: `floor(trunc(dpi) / 0.0254)` with
a small epsilon so 254 gives 10000; `phys_ppm_per_cm`: `trunc(px/cm) * 100`).

## JPEG density (exports)

JFIF density is always written **per inch**, truncated. A pixels/cm document is converted
first (`px/cm * 2.54`):

| Resolution | JFIF density |
|---|---|
| 72 ppi | 72 |
| 72.5 ppi | 72 |
| 150 ppi | 150 |
| 254 ppi | 254 |
| 28.99 px/cm (73.63 ppi) | 73 |
| 28.35 px/cm (72.009 ppi) | 72 |

FireCraft: `firecraft_io::jfif_dpi` (it used to round, 72.5 -> 73). FirePixel rounds
(`params.dpi.round()`); whether CS6 rounds or truncates is unmeasured.

## Reading

* **Fireworks PNG**: `RESf` and `RSUi` from the tree; `pHYs` is ignored.
* **Flat PNG** (File > Open, 2026-11-10): `pHYs` is shown in **pixels/cm**, `ppm / 100`
  (5905 px/m -> 59.05 pixels/cm, `RSUi` 1). A 2834 px/m file therefore opens as 28.34 px/cm,
  not 72 ppi.
* **JPEG**: JFIF density (300 -> 300 pixels/inch). **TIFF**: XResolution (200 -> 200).
  **Targa, BMP**: 72 ppi. See [opening-files.md](opening-files.md).
* **Flat PNG without `pHYs`**: File > New from the clipboard shows 28.34 pixels/cm, Fireworks'
  72 ppi as `pHYs` gives it (2834 px/m). FireCraft's File > Open gives such a file 72 ppi;
  what Fireworks' File > Open does with it is unmeasured.

## File > New

### Starting values from the clipboard

With a picture on the clipboard, the dialog starts from its size and resolution, in the
picture's own unit (details in [clipboard.md](clipboard.md)):

| Clipboard | Resolution shown |
|---|---|
| Fireworks PNG | its `RESf`/`RSUi` (2540 Pixels/Inch; 28.9899 Pixels/cm), not its `pHYs` |
| PNG with `pHYs` | `ppm / 100` Pixels/cm (5906 -> 59.06) |
| PNG without `pHYs` | 28.34 Pixels/cm |
| nothing | the last document's settings |

FireCraft: `firecraft_io::clipboard_image_info` (a DIB's pixels/metre is read like `pHYs`;
unmeasured in Fireworks, which under Wine only reads the clipboard's PNG).

### Changing the unit

Changing Pixels/Inch <-> Pixels/cm in the dialog **converts the number**. Measured on 39
conversions in Fireworks MX; the same 22 checked in Fireworks CS6 gave identical results:

1. Compute in **float32**: multiply or divide by 2.54.
2. A result within **0.02** of a whole number becomes that whole number.
3. Anything else is rounded to two decimals and shown with two (37.80, 25.40).

The dialog keeps the rounded number: switching back converts it again, and OK stores it
(72 ppi switched to cm and confirmed gave `RESf` 28.35, `RSUi` 1). **Typed values are stored
as typed** (73.6143 -> `RESf` 73.614304).

| From | Typed | Exact | Shown |
|---|---|---|---|
| inch | 72 | 28.3465 | 28.35 |
| inch | 96 | 37.7953 | 37.80 |
| inch | 150 | 59.0551 | 59.06 |
| inch | 300 | 118.110 | 118.11 |
| inch | 254 | 100 | 100 |
| inch | 72.5 | 28.5433 | 28.54 |
| inch | 71.99 | 28.3425 | 28.34 |
| inch | 73.6 | 28.9764 | 28.98 |
| inch | 73.63 | 28.9882 | 29 |
| inch | 73.6117 | 28.9810 | 29 (0.0190 from 29) |
| inch | 73.6092 | 28.9800 | 28.98 (0.0200005 in float32; float64 would snap) |
| inch | 50.79 | 19.9961 | 20 |
| cm | 28.35 | 72.009 | 72 |
| cm | 28.34 | 71.9836 | 72 (two decimals would give 71.98) |
| cm | 28.99 | 73.6346 | 73.63 |
| cm | 29 | 73.66 | 73.66 |
| cm | 10 | 25.4 | 25.40 |
| cm | 37.8 | 96.012 | 96 |
| cm | 59.06 | 150.012 | 150 |
| cm | 28.3 | 71.882 | 71.88 |
| cm | 28.3386 | 71.9800 | 72 (0.01996 from 72) |
| cm | 28.3378 | 71.9780 | 71.98 |
| cm | 28.355 | 72.0217 | 72.02 |

Chained: 28.99 cm -> 73.63 in -> 29 cm -> 73.66 in, because each switch converts the shown
(rounded) value.

The field accepts **1 to 100000** ("Please enter a number between 1 and 100000."), so 1 ppi
switched to cm (0.39) is refused.

FireCraft: `firecraft_io::convert_resolution_unit`, the dialog's Pixels/Inch | Pixels/cm
choice, range 1 to 100000; the unit is remembered with the size (`new_doc=...@28.35cm:...` in
the preferences). FirePixel's dialog only relabels the number (72 Pixels/Inch becomes "72
Pixels/cm") and makes a Pixels/cm document as pixels/inch (`resolution * 2.54`, no `perCm`).

## Clipboard

Fireworks MX's clipboard PNG carries the source document's resolution (a 150 ppi document's
copy: `RESf` 150, `RSUi` 0, `pHYs` 5905). FireCraft's carries it too
(`firecraft_io::clipboard_png(..., res)`), so objects pasted between the two keep it. Fireworks
MX asks "The pasted image has a different resolution. Resample the image to match the
document?" when a paste's resolution differs from the document's (it asked for plain PNGs
without `pHYs` and for 2835 px/m into a 72 ppi document). FireCraft never resamples a paste,
which matches answering "Don't Resample".

## Image Size (Modify > Canvas)

Measured 2026-11-16 (the dialog itself: [canvas-size.md](canvas-size.md#image-size)).

**Fireworks MX's Image Size dialog always reads `RESf` as pixels/inch** and uses `RSUi` only
to choose the unit it shows: a document made by script at 28.35 px/cm (`RESf` 28.35,
`RSUi` 1) shows 11.161 Pixels/cm there (28.35 / 2.54). That disagrees with New Document and the
`pHYs` writer, which read `RESf` in the document's unit.

Switching the dialog's unit converts the number exactly and shows 3 decimals (72 ppi ->
28.346, 108 -> 42.52 Pixels/cm), but OK with only the unit changed **stores the old number
with the new unit**: a 108 ppi document became `RESf` 108, `RSUi` 1, and its saved `pHYs` was
10800 (108 px/cm, about 274 ppi, to other programs). Reopened, Fireworks' own dialog still
shows 42.52 Pixels/cm. A resolution typed in the dialog (resample on, 108 ppi) is stored as
typed (`RESf` 108, `RSUi` 0).

FireCraft doesn't copy this: its Image Size keeps the resolution in the document's unit, as New
Document and `pHYs` do (72 ppi switched to cm stores 28.346457 px/cm), so the file is right for
other programs and for Fireworks' New Document and saving. Fireworks' Image Size dialog will
show such a pixels/cm document divided by 2.54, as it shows its own.

FirePixel's Image Size dialog converts on a unit change (without Fireworks' rounding), then
sends only pixels/inch and leaves the document's unit flag unchanged: 100 Pixels/cm on an inch
document is saved as 254 Pixels/Inch (seen by the user in Fireworks MX, 2026-11-16), and, from
its code, a pixels/inch value would be written into a pixels/cm document unconverted.

## Open questions

* Whether Fireworks' Image Size, with a resolution typed in Pixels/cm, stores it as typed
  (and so as pixels/cm in `pHYs`) or converts it.
* Fireworks' File > Open of a flat PNG without `pHYs` (72 ppi or 28.34 px/cm?).
* CS6's JPEG density (truncated like MX, or rounded as FirePixel does) and its resolution
  range.
* The resolution of a DIB on the Windows clipboard in File > New.

## Measuring

* Documents at a given resolution: `fw.createFireworksDocument({x, y}, {pixelsPerUnit,
  units: "inch" | "cm"}, "#ffffff")` in a `.jsf` passed to Fireworks (other unit names such as
  "centimeter" fail with error -2147216343), then `fw.saveDocument` and read `RESf`/`RSUi`
  (`crates/fwpng/examples/dump_tree`) and `pHYs`.
* Exports: set `dom.exportOptions.exportFormat` ("PNG", "JPEG") and `colorMode` ("32 bit",
  "indexed", "24 bit"), `dom.setExportOptions`, then `fw.exportDocumentAs(dom, url, null)`.
* The New Document dialog (MX at 1280x1024 on Xvfb: resolution field at 582,488, unit list
  arrow at 738,488, options at y 506 inch / 520 cm; CS6 at 1600x1200: 470,325, 610,325, options
  344 / 357). Clear the field with End plus BackSpaces, not a double-click: a Win32 edit's
  double-click selects only the part of the number before or after the '.'.
