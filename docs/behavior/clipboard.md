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
own included, and centres the objects in view as it does a picture (below). Pasting Fireworks to
Fireworks starts from the copied position because the private format comes first; decoding it
is on the roadmap (lowest priority).

## Where a paste lands

Measured 2026-11-27 in Fireworks MX (JSF `clipCopy`/`clipPaste` of rectangles into new
documents; the document window maximized, showing the top-left 827 x 796 of a 2000 x 1500
canvas; `addNewRectangle` 50 x 40 unless noted):

| Copied at | Pasted into | Lands at |
|---|---|---|
| (100, 100) | 2000 x 1500 | (100, 100): in view, kept |
| (800, 100), partly in view | 2000 x 1500 | (800, 100) |
| (-20, 100), partly off the canvas | 2000 x 1500 | (-20, 100) |
| (100, 100), 1000 x 800 | 2000 x 1500 | (100, 100) |
| (1500, 1200) | 2000 x 1500 | (388, 378): centred in view |
| (835, 100), just past the view | 2000 x 1500 | (388, 378) |
| (-100, 100), off the canvas | 2000 x 1500 | (389, 378) |
| (1500, 1200), 1000 x 800 | 2000 x 1500 | (-87, -2): centred though bigger than the view |
| (500, 400) | 300 x 200 (smaller than the window) | (125, 80): centred on the canvas |
| (100, 50) / (280, 50) | 300 x 200 | kept |
| (500, 400), 400 x 300 | 300 x 200 | (-50, -50) |
| (300, 250) | 50 x 40 (File > New from the clipboard) | (0, 0) |

2026-11-28, Select All + Copy in `bluesquares.png` (two 42 x 53 bitmaps, 107 x 82 together;
the clipboard picture is 107 x 82), pasted into new documents: **107 x 82: moved to the corner**
(the bitmaps at (0, 0) and (65, 29)); 106 x 82, 108 x 82, 107 x 81, 107 x 83, 100 x 100,
120 x 90, 200 x 200, 2000 x 1500: kept where they were. A 50 x 40 rectangle with a drop shadow
(clipboard picture 58 x 48, from the path's corner) pasted into 58 x 48: the path at (0, 0);
50 x 40: (-4, -4); 57 x 47: (-1, -1); 200 x 150 in a tiny window: centred in view.

So, measured by the copy's **picture** (the clipboard image, effects included):

1. A canvas exactly the picture's size gets the picture at (0, 0). File > New offers that size,
   so Copy, New, Paste fills the new canvas.
2. Otherwise objects that overlap the **visible canvas** (the part of the canvas the window
   shows) stay where they were copied, even partly in view or partly off the canvas.
3. Otherwise they move by `round(visible centre - picture centre)`, halves away from zero (the
   388 / 389 pair; the -0.5 of 57 x 47), even when bigger than the view.

A picture from another application goes to the centre of the visible canvas the same way
(101 x 61: (363, 368) on the 2000 x 1500 canvas, (100, 70) on 300 x 200). In a small document
window the visible canvas is small: a headless run with a tiny window put a rectangle at
(72, 12) in a 200 x 150 canvas, and the old (80, 20) above.

JSF reports bluesquares' bitmaps at (-7, -13) and (58, 16), while they show (and FireCraft
reads them) at (40, 46) and (105, 75): an `Image`'s `left`/`top` there is in another frame,
the offsets between objects agree.

FireCraft does the same (`DocState::paste`, `visible_canvas`; the canvas view records
`DocState::view_rect` as it draws; `firecraft_io::pasted_image_origin`), also for Fireworks
PNGs from the system clipboard and copied files. The picture is `Clipboard::picture`: for
FireCraft's own copy what the objects draw, rounded out (`picture_rect`, below; the clipboard
PNG and File > New use it too), for a Fireworks PNG its `XLCf`/`YLCf` and size
(`firecraft_io::clipboard_picture`). Before a document is first drawn the whole canvas counts
as visible.

## How big a copy's picture is

Measured 2026-11-28: copy one object, File > New (a script's `fw.createDocument()` after a copy
takes the clipboard's size too), paste (the canvas is the picture's size, so the picture lands
at (0, 0) and the object's place in it is the room the picture adds). 50 x 40 rectangles:

| Effect | Room added (left, top, right, bottom) |
|---|---|
| Drop Shadow 315 deg, distance 7, softness 4 | 0, 0, 8, 8 |
| Drop Shadow 135 deg, 7, 4 | 8, 8, 0, 0 |
| Drop Shadow 30 deg, 10, softness 0 | 0, 4, 8, 0 |
| Drop Shadow 0 deg, 0, softness 10 | 10 all round |
| Glow width 5, softness 12 (bluesquares2, glowpx too) | 17 all round |
| Glow offset 3, width 2, softness 6 | 11 all round |
| Outer Bevel width 10 | 10 all round |
| Gaussian Blur radius 3 | 8 all round |
| Inner Shadow, Inner Glow, Inner Bevel | none |
| 1-pixel pencil stroke (lines.png) | half a pixel, then rounded out |

So the picture is the object's bounds, a path's grown by its stroke, then each effect in turn
adds what it draws outside: a shadow is the object moved by its offset (truncated:
`firecraft_raster::effects::shadow_offset`) and grown by its softness, a glow grows by offset +
width + softness, an Outer Bevel by its width, a Gaussian blur by its reach. Bounds rounded out
to whole pixels. FireCraft: `Element::picture_bounds`, `Effect::extent`, `stroke_fw::reach`;
`cargo run -p firecraft-io --example picture_rooms -- FILE...` prints each object's picture and
room, to compare with these measurements.

The 30-degree shadow: Fireworks MX draws it 4 pixels up (fxprobe's shadow starts at row 36 for
a rectangle at row 40), as its copy says; 10 sin 30 deg is 4.999... in doubles. FireCraft
computed the offset in single precision (5) until 2026-11-28; in doubles the probe's mean
difference from Fireworks' render went from 0.19 to 0.15.

Not matched yet (FireCraft's picture differs):

* Gaussian blurs of large radius: ball.png's radius-130 bitmap blur copies 36, 4, 36, 4 (a
  265 x 265 picture for 193 x 257); circleBlur's radius 220 175, 158, 175, 159. FireCraft adds
  its blur reach all round (175, or nothing above radius 200).
* lines2's strokes: the natural brush 60, 71, 78, 78 (FireCraft 80 all round); the second
  matches (9 all round); the third 4, 5, 5, 5 (FireCraft 5 all round).
* Symbol instances count what their symbol draws (foo3's matches: 1420 x 655), but box.png's
  instance copies at 486 x 500 where its symbol's geometry scaled is 403 x 444, and folds'
  is one pixel wider on the left.

## How FireCraft uses it

* Copy/Cut: FireCraft keeps the objects itself and also puts a Fireworks PNG of them on the
  system clipboard as `PNG` (Windows) / `image/png` (X11) — `firecraft_io::clipboard_png`,
  `firecraft-sysclip`. Other applications paste it as a picture. On Windows a `CF_DIB` of the
  objects' picture (over white, `firecraft_io::encode_dib`) follows "PNG", as Fireworks MX
  puts one: Chrome on Windows offered a page no picture with "PNG" alone (Ctrl+V did nothing,
  `navigator.clipboard.read` listed no image; the user, 2026-11-22), while objects copied
  in Fireworks MX (with its DIBs) pasted into the web version as objects. Fireworks MX still
  pastes the "PNG" as objects with the DIB beside it (measured under Wine 2026-11-22:
  `sysclip_probe.exe set FILE SECS DIB`, then `dom.clipPaste()` gave the file's Rectangle
  and Text, not the DIB's green square).
* Paste: if the system clipboard holds a picture that FireCraft didn't put there, it is read
  with `firecraft_io::read_clipboard`: a Fireworks PNG becomes its objects, placed as
  [above](#where-a-paste-lands) (symbols come along, renumbered if their ids clash); any other
  picture becomes a bitmap object centred on the visible canvas. Otherwise FireCraft pastes its
  own last copy.
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

Changing the dialog's unit converts the number with Fireworks' own rounding; see
[resolution.md](resolution.md).

The canvas color stays as last used. FireCraft does the same
(`firecraft_io::clipboard_image_info`; DIBs by their pixels/metre like PNG, unmeasured) and has
the same Pixels/Inch | Pixels/cm choice. Its additions: a copied image file gives its size and
the resolution File > Open would give it; in the browser its own copied objects give their
bounds.

## Resolution

The clipboard PNG carries the source document's resolution and unit, in Fireworks MX's and in
FireCraft's, and `pHYs` follows Fireworks' truncating rule; see [resolution.md](resolution.md).

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
  pasting it back pastes FireCraft's own copy. Pictures from other applications paste as on
  the desktop (a bitmap centred on the canvas; PNG, JPEG, BMP, TIFF, GIF, WebP decoded):
  * Ctrl+V: eframe passes on only a paste event's text, so `apps/firecraft-web` listens for
    `paste` on the document in the capture phase and takes the first `image/*` file
    (`clipboardData.files`, else `items`). It takes it only while a document is open and no
    text is being typed (`Platform::set_accepts_pictures`, from `egui_wants_keyboard_input`);
    then eframe doesn't see the event, and a text block or text field being typed in gets the
    paste's text instead. No permission prompt.
  * Edit > Paste, the context menus' Paste, Paste as Mask and Paste Inside: read the clipboard
    with the Clipboard API (`navigator.clipboard.read`; Chrome asks for permission once,
    Safari and Firefox show their own Paste button each time). With no picture there, the
    paste uses FireCraft's own copy; the menu items are enabled whenever the API exists.
    Browsers without it (Firefox before 127) paste only FireCraft's own copy from the menus.
  * Tested 2026-11-22 in headless Chromium (CDP: a synthetic `ClipboardEvent` with a PNG file,
    and `navigator.clipboard.write` then Edit > Paste).
* Pasting text outside a text block isn't built (Fireworks MX makes a text object of it); the
  status bar says to paste into a text block with the Text tool.

D-Bus plays no part: the Linux clipboard is the X11 selection (or Wayland's data-device
protocol), which Wine bridges to the Windows clipboard.
