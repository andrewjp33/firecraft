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
* Paste: if the system clipboard holds a PNG that FireCraft didn't put there, it is read with
  `firecraft_io::read_clipboard_png`: a Fireworks PNG becomes its objects at their original
  place (symbols come along, renumbered if their ids clash); a plain PNG becomes a bitmap.
  Otherwise FireCraft pastes its own last copy.
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
