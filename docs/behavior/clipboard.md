# Clipboard (Cut/Copy/Paste with Fireworks MX)

## What Fireworks MX puts on the clipboard

Measured with `tools/fwmx-oracle/xclip.py` (X11 side) and a Windows-side lister run under Wine
(`EnumClipboardFormats`) after `dom.clipCopy()` on a named rectangle:

| Format | Contents |
|---|---|
| `PNG` (registered) | A complete Fireworks PNG (mkTS/mkBT chunks): a document cropped to the copied objects (50x40 for a 50x40 rectangle), root `XLCf`/`YLCf` = their original top-left, objects with names, effects, fills. Its flat image is the objects' picture. |
| `Fireworks Internal Clipboard Format 3.0` | Private; provided through OLE (not readable as a memory block). |
| CF_DIB, CF_DIBV5, CF_BITMAP | The picture for other applications. |
| OLE wrappers | `Ole Private Data`, `DataObject`, Wine's marshalled data object. |

Fireworks MX **pastes the `PNG` format back as editable objects** when it's the only format: its
own clipboard PNG and a FireCraft-written document (18 objects with live effects) both pasted as
paths with their effects. It **ignores the stored position** (`XLCf`/`YLCf`) of that format, its
own included: pasted into a 500x400 document, a rectangle copied from (10, 10) lands at (80, 20)
either way. Pasting Fireworks to Fireworks keeps positions because the private format comes
first; decoding it is on the roadmap (lowest priority).

## How FireCraft uses it

* Copy/Cut: FireCraft keeps the objects itself and also puts a Fireworks PNG of them on the
  system clipboard as `PNG` (Windows) / `image/png` (X11) — `firecraft_io::clipboard_png`,
  `firecraft-sysclip`. Other applications paste it as a picture.
* Paste: if the system clipboard holds a picture that FireCraft didn't put there, it is read
  with `firecraft_io::read_clipboard`: a Fireworks PNG becomes its objects at their original
  place (symbols come along, renumbered if their ids clash); any other picture becomes a bitmap
  object centred on the canvas (below). Otherwise FireCraft pastes its own last copy.
* Pictures from other applications (screenshots, browsers, image editors): `firecraft-sysclip`
  reads, in this order, Windows `PNG`, `CF_DIB`, `CF_DIBV5`, `JFIF`; X11 `image/png`,
  `image/bmp` (`image/x-bmp`, `image/x-MS-bmp`), `image/jpeg` (the first the owner lists in
  `TARGETS`). Print Screen and most Windows programs offer only DIBs. `CF_DIB` goes before
  `CF_DIBV5`: at 32 bits a DIB's fourth byte is officially unused, so `firecraft_io::decode_dib`
  reads it as alpha only when the header has an alpha mask (V4/V5 header, `BI_ALPHABITFIELDS`),
  and treats alpha that is 0 everywhere as opaque. Programs with real alpha (browsers, editors)
  also offer `PNG`, which comes first. Wine passes a Windows program's `CF_DIB` to X11 as
  `image/bmp` and an X11 `image/bmp` to Windows programs as a 32-bit `BI_RGB` `CF_DIB` with
  the fourth byte 0 (checked 2026-11-16 with `sysclip_probe.exe` under Wine; decoded exactly).
  RLE-compressed DIBs aren't read.
* Copied image files (FireCraft's addition): with no picture on the clipboard, Paste takes the
  files copied in Explorer (`CF_HDROP`) or a Linux file manager (`text/uri-list`, `file://`
  URIs percent-decoded) whose extension File > Open accepts, and pastes each as File > Open
  reads it (`firecraft_io::file_as_paste`): a picture as one bitmap, a Fireworks PNG as the
  objects of its visible layers (frame 1), the file's canvas centred like a pasted picture; one
  undo step per file. Fireworks MX doesn't do this: with only a file on the clipboard (X11
  `text/uri-list`, which Wine offers as `CF_HDROP`) its Edit > Paste stays disabled
  (2026-11-16). Checked: Wine turns `text/uri-list` into a `CF_HDROP` that `sysclip_probe.exe
  files` lists with Windows paths (spaces included).

## File > New starts from the clipboard

Fireworks MX's New Document dialog starts from the clipboard's picture (measured 2026-11-16),
in the picture's own resolution unit:

| Clipboard | Width x Height, Resolution |
|---|---|
| PNG without `pHYs`, 101 x 61 | 101 x 61, 28.34 Pixels/cm (its 72 ppi as `pHYs` gives it: 2834 px/m) |
| PNG, `pHYs` 5906 px/m | 123 x 45, 59.06 Pixels/cm |
| Fireworks PNG of a 2540 ppi document (`pHYs` 100000) | 40 x 30, 2540 Pixels/Inch (from `RESf`/`RSUi`) |
| Fireworks PNG of a 28.99 px/cm document (`pHYs` 2800) | 40 x 30, 28.9899 Pixels/cm |
| nothing (a copied file included) | the last document's settings |

Changing the dialog's unit converts the number (measured 2026-11-16, 39 conversions,
`firecraft_io::convert_resolution_unit`): in float32, times or divided by 2.54; a result within
0.02 of a whole number becomes that number, anything else is rounded to two decimals and shown
with two (37.80). The rounded number is what the dialog keeps: switching back converts it
(28.99 px/cm -> 73.63 ppi -> 29 px/cm), and OK stores it (72 ppi switched to cm gave `RESf`
28.35, `RSUi` 1). Typed values are stored as typed (73.6143 -> `RESf` 73.614304). Examples:
72 ppi -> 28.35; 28.34 px/cm -> 72 (71.98 at two decimals); 73.6117 ppi -> 29; 73.6092 ppi ->
28.98 (0.0200005 from 29 in float32; in float64 it would snap). The field accepts 1 to 100000
("Please enter a number between 1 and 100000."), so 1 ppi switched to cm (0.39) is refused.
Fireworks CS6 converts identically (the same 22 conversions checked 2026-11-16, edge cases
included). FirePixel's dialog doesn't convert: it only relabels the number, and makes a
Pixels/cm document as pixels/inch (value x 2.54).

The canvas color stays as last used. FireCraft does the same
(`firecraft_io::clipboard_image_info`; DIBs by their pixels/metre like PNG, unmeasured) and has
the same Pixels/Inch | Pixels/cm choice. Its additions: a copied image file gives its size and
the resolution File > Open would give it; in the browser its own copied objects give their
bounds.

## Resolution in PNG and JPEG data

Fireworks MX keeps the editable resolution in the tree (`RESf` value, `RSUi` 1 = pixels/cm)
and writes `pHYs` (pixels/metre, whole numbers) in saved files, exports and its clipboard PNG
by truncating the resolution to a whole number in its own unit first: pixels/inch
`trunc(ppi) * 10000 / 254` (integer division), pixels/cm `trunc(px/cm) * 100`. Measured
2026-11-16 with documents made by script and saved: 72, 72.5, 72.01 ppi -> 2834; 71.99 -> 2795;
96 -> 3779; 150 -> 5905; 254 -> 10000 (a float `* 39.37` would give 9999); 2540 -> 100000;
28.35, 28.3465, 28.99 px/cm -> 2800; 37.8 -> 3700. PNG 8 and PNG 32 exports write the same.
JPEG exports write JFIF density in dots per inch, truncated: 72.5 ppi -> 72, 28.99 px/cm -> 73
(73.6), 28.35 px/cm -> 72. FireCraft writes the same (`firecraft_io::png_ppm`, `jfif_dpi`; it
used to round, writing 2835 for 72 ppi, and ignored the pixels/cm unit). FirePixel measured
the same `pHYs` rule on CS6 but rounds the JPEG density.

Its clipboard PNG carries the source document's resolution (a 150 ppi document's copy:
`RESf` 150, `RSUi` 0, `pHYs` 5905), and so does FireCraft's, so objects pasted between the two
keep the document's resolution (Fireworks MX asks to resample a paste whose resolution differs
from the document's).

## Where Fireworks MX puts a pasted picture

Measured 2026-11-16 (X11 `image/png` -> Wine `PNG` -> Edit > Paste into a 300x200 document):
the picture becomes a bitmap object **centred on the canvas**, halves rounded away from zero,
and the canvas keeps its size:

| Picture | Lands at |
|---|---|
| 100 x 60 | (100, 70) |
| 101 x 61 | (100, 70) |
| 303 x 203 | (-2, -2) |
| 500 x 400 | (-100, -100) |

FireCraft does the same (`firecraft_io::pasted_image_origin`). MX first asked "The pasted image
has a different resolution. Resample the image to match the document?" for these PNGs (no
`pHYs`, or 72 dpi as written by PIL); FireCraft never resamples (MX's "Don't Resample").
Under Wine, MX only took the `PNG` format: offered `image/bmp` alone, Wine never requested it.
* Linux: Fireworks under Wine is an X11 client, so FireCraft uses the X11 CLIPBOARD (directly,
  or through XWayland on a Wayland desktop). Wine exports `PNG` as `image/png` once its
  `explorer` process runs (always in a desktop session; under a bare Xvfb it has to be started).
  Large data uses the INCR protocol (tested with a 5 MB document).
* The native app turns egui's clipboard shortcuts off and handles Ctrl+X/C/V itself; text
  fields get them back through `raw_input_hook` (`FireCraftApp::text_clipboard_keys`).
* Browser build: no system clipboard for objects; Copy puts a short note on the clipboard and
  pasting it back pastes FireCraft's own copy.

D-Bus plays no part: the Linux clipboard is the X11 selection (or Wayland's data-device
protocol), which Wine bridges to the Windows clipboard.
