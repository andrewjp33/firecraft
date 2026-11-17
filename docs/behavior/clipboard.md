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
  RLE-compressed DIBs and copied image *files* (`CF_HDROP`, `text/uri-list`) aren't read yet.

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
