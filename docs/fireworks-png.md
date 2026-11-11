# Fireworks PNG format notes

Derived black-box from files written by Fireworks MX (its shipped libraries/styles and documents it
saved under the oracle harness), then confirmed by having Fireworks MX open files FireCraft wrote.
Implementation: `crates/fwpng` (container) and `crates/io` (document mapping).

## Container

A normal PNG whose `IDAT` is the flattened canvas (32-bit RGBA), so every viewer shows it, plus:

| chunk | content |
|---|---|
| `mkBF` | 72 bytes: `FADECAFE 00000004` then zeros |
| `mkTS` | zlib: the document tree (below) |
| `mkBS` | zlib: a small secondary tree (one layer, one 256-wide image); purpose unknown, optional |
| `mkBT` | one pixel tile each |
| `prVW` | zlib: preview, magic `CAFEBEEF`; optional |

Fireworks MX also writes `sBIT` and `tEXt Software=Macromedia Fireworks MX`; neither is required.

## Tree grammar

`TAG KIND { body }` with a 3-byte ASCII tag and kind `v` (vector of nodes), `i` (hex integer,
32-bit two's complement), `f` (decimal), `b` (`0`/`1`), `s` (u16 big-endian length in UTF-16 units,
then UTF-16BE; may contain `}`). Every observed file round-trips byte for byte through FireCraft's
parser/serializer (8 trees, 258 distinct tags).

## Tiles (`mkBT`)

`FACECAFE`, u32 id (referenced by `TIDi`), u32 flags (0/1, preserved), 64 zero bytes, zlib payload.
Image tiles (`TSZi{80}`, 128 px) inflate to 128×128 **A,R,G,B** straight alpha; mask tiles
(`TSZi{100}`) to 256×256 bytes. Tiles cover an element row-major in `TILv` order.

## Document

```text
MKBv  FRCi frames, RQNi ?, XLCf/YLCf canvas origin, WIDf/HITf size, RESf, RSUi (1 = per cm),
      BGCi canvas ARGB (alpha 0 = transparent),
      [PLLv brushes, FLDv fills, GLDv gradients, EFDv effects, MSLv masks/textures, PATv, EXPv export,
       MNFv]  preset lists: optional
      LYLv  LAYv*  (bottom layer first)
      GRDv grid, GDSv guides, OSBi, OSAi
LAYv  BKGb, URLb (web layer), LNMs name, MSLb (unknown; 1 on the Web Layer), MCLb (1 = own content per
      frame, 0 = shared across frames: Fireworks then shows cell 0 in every frame), DISb (expanded),
      CLLv  CELv* (one per frame)  { LCKb, ELMv elements }, VIFv { VISb }
IMGv  XOFf YOFf XLCf YLCf (document space), WPXi HPXi, TSZi, TILv { TIDi* }
      pixel buffer/tile grid at (XLC, YLC) sized WPX x HPX; visible image rect is
      (XLC+XOF, YLC+YOF)..(XLC+WPX, YLC+HPX): XOF/YOF hide the buffer's top-left strip
      (Fireworks MX's bitmap constructor 0x62acd0; reader 0x7b4ff9)
PTHv  PATv { BCLi stroke ARGB, FCLi fill ARGB, BPLv brush preset, FPLv fill preset, TXBv/TXFv textures },
      BRPi, PTTi, FOTb, RNDi, TOXf.., PSXf..PFYf (gradient handles), EFLv effects,
      PBLv { PBPv* { ISCb closed, BSLf, PPCi, PBTv* { XLCf YLCf [XPCf YPCf] [XSCf YSCf] ... } } },
      PCLv (cached flattened stroke polyline with pressure)
TXBv  (stroke texture; TXFv fill texture) INMs name, MSKv { WPXi HPXi TSZi{100} TILv { TIDi* } }:
      8-bit mask = 255 - grey of the texture image, 256 x 256-byte tiles with tile flags 1
      (MX refuses mask tiles with flags 0); Fireworks draws from this mask, not its folder.
      TOXf/TOYf on the path: texture origin (document space)
TXTv  text attributes (FONs, PTSf, BOLb...), TFSv runs, cached IMGv
GRPv  GRTi, ELMv
(any) OBNs object name (Layers panel), after the object's own data, before BLDi/EFLv/OPAi;
      VISb (first child) hidden; BLDi blend mode; OPAi opacity in tenths of a percent
      LCKb locked (Fireworks CS's per-object lock, after VISb; CS writes it on every object,
      some MX objects carry LCKb 0; MX ignores it). FireCraft writes it when set or as read.
MSLv  (root) MSTv* symbol definitions: UNMs name, MSNi id, MSYi type (absent Graphic, 1 Button
      with FRCi 4 states, 2 Animation), MOXf/MOYf, LUTs, own LYLv (Layer 1 + Web Layer), GRDv,
      GDSv, FRCi frames, OSBi, OSAi, BGCi, ATSb 1, IDSb, DVLb, IODb; LFNs/LMNi when linked to a
      library file. Convert to Symbol centres the content on the symbol origin (a 50x40
      rectangle at (10,10) becomes -25..25 x -20..20; the instance matrix translates by (35,30))
ALSv  symbol instance: MSNi (symbol id), ALNi (buttons), MTXv (M00 M01 M10 M11 M20 M21: symbol
      space -> document space, row-vector convention), XFMi 0, ANNi (= the symbol's frame
      count for new instances), ANXf 0, ANYf 0, ASCf 100 (not yet decoded further)
```

Bezier control points (`XPC/YPC` predecessor, `XSC/YSC` successor) are **absolute** coordinates and
are omitted when they coincide with the anchor.

## What Fireworks MX requires (bisected with `tools/fwmx-oracle/fw_inspect.sh`)

* Every tile id referenced anywhere in the trees must exist, or Fireworks silently drops content
  (a whole layer, in one test). FireCraft therefore carries over every original tile still
  referenced by preserved nodes.
* A path needs **`RNDi`** (per-object id) and **both `BCLi` and `FCLi`** in `PATv`, even with no
  stroke or no fill. Without them the file fails to open (error `0x8004142B`).
* **Child order matters for paths.** MX reads a path's effects, blend mode and opacity only in
  its header: `VISb LCKb PATv BRPi PTTi FOTb EFLv RNDi TOXf TOYf BLDi OPAi PCFb PSX..PFY PBLv
  PCLv PRIv ... OBNs`; after the points they are ignored (FireCraft wrote opacity there, so MX
  showed FireCraft paths fully opaque until 2026-11-05). Bitmaps: `... TILv BLDi OPAi EFLv
  OBNs`. Verified by letting MX set opacity, blend mode and name (`setOpacity`, `setBlendMode`,
  `setElementName`) and reading its save.
* `OPAi` = floor(byte * 1000 / 255) of Fireworks' opacity byte; reading, byte =
  round(OPAi * 255 / 1000) (500 and 501 -> 128, 498 -> 127).
* Not required: preset lists, `prVW`, `mkBS`, `sBIT`, `tEXt`, `PCLv`, textures, gradient handles,
  `BZLv`/`NSDf` on points, `RQNi`.

## Shape primitives (PRIv)

A rectangle drawn with Fireworks' Rectangle tool is a *RectanglePrimitive*: its `PTHv` has
`PTTi 1`, `TOXf`/`TOYf`, `PCFb`, `PSXf..PFYf` after `RNDi`, and after the points a
`PRIv { PCFb, PSXf..PFYf, MTXv (row vectors: M20/M21 translation), RCTv { RDSf roundness,
LFTf, TOPf, RITf, BOTf base box } }` (CS adds `RDMb`). Fireworks MX and CS rebuild the shape
from `RCTv` and `MTXv`, not from the path points (verified with tools/fwmx-oracle: moved
points with the old record reopen at the old place).

Roundness `RDSf` (0..1) is the corner radius as a fraction of half the shorter side (hd.png:
0.3 on a 209x123 box gives radius 18.45). The outline: 4 corners clockwise from top left, or
with rounding 8 points from the left end of the top edge, each corner a quarter circle with
handles 0.5523 of the radius.

FireCraft keeps the record as `Path::rect` (`RectShape`: base box, roundness, matrix). Moves
and affine transforms update its matrix; editing points makes the path plain (no `PRIv`). On
read and write, a record that doesn't match the points (files saved by FireCraft before it
kept primitives) is fitted to them by an affine transform, or dropped. FireCraft's Rectangle
tool draws primitives, which Fireworks MX opens as RectanglePrimitive objects (verified:
sharp, rounded, and rotated rounded, rendering the same as FireCraft).

## Fireworks CS files

A Fireworks CS5 tree holds two documents: `MKBv` (the classic one, which Fireworks MX reads;
in CS files its root also has `VRSi`/`VRMi` version fields and `PIDs`, `PILb`, `ILLb`, `IMPb`,
`LSMv`, `ZSLv`), followed by a second top-level node, `PDCv` (`VRSi`, `VRMi`, then the whole
document again, CS's own copy). FireCraft reads `MKBv` and writes only it; it removes the
CS-only root fields, since with them CS expects the `PDCv` and opens an empty canvas (seen on
a re-saved CS5 file). Without them CS reads the file like one from MX.

9-slice guides (`NGSv` in a symbol, see `docs/behavior/symbols.md`) are read by CS. CS never
skews or distorts a 9-slice instance (it asks to remove the slices) and hangs opening a file
that has one; for a symbol with such an instance FireCraft writes the guides in its own node
`FNGv` (same `NGCf` values) instead, which MX and CS ignore, so CS shows it unsliced.

## Clipboard

Fireworks MX copies objects as a clipboard format named **"PNG"** holding a complete Fireworks
PNG cropped to the copied objects, with the root `XLCf`/`YLCf` set to their original top-left
(also "Fireworks Internal Clipboard Format 3.0", CF_DIB/CF_DIBV5 and OLE wrappers). It pastes
"PNG" back as editable objects, including FireCraft-written ones. Under Wine the format is the
X11 target `image/png` (with Wine's `explorer` running, as in a desktop session); see
`docs/behavior/clipboard.md`.

Fireworks MX renders FireCraft-written documents the same as FireCraft (strokes: see
`docs/behavior/strokes.md`).

## Compatibility levels (design)

Fireworks MX opens files from later versions only when they keep the MX structure; CS-era files
(pages `PDCv`, `VRSi`/`VRMi` format version, CS frame/state layout) generally don't open in MX,
while MX files open fine in CS versions. Measured with the oracle (`fw_inspect.sh`):

* MX **ignores unknown nodes** anywhere in the tree (root vectors and scalars, inside elements,
  brand-new tags) and opens the file normally.
* MX **drops unknown nodes when it re-saves** a file.

So FireCraft saves in tiers:

1. **Fireworks MX (default).** MX structure. Features MX lacks are either *baked* into ordinary
   objects (e.g. the 9-slice Scaling tool reshapes paths/bitmaps directly) or stored as extra nodes
   MX ignores, using the CS tag names where CS defines them (e.g. 9-slice guides on a symbol as
   `NGSv{NGLb,NGKi,NGCf x4 (top, bottom, left, right),NGMv,NGVv}`). The file opens in MX and CS;
   editing and re-saving in MX loses only the extras.
2. **Fireworks CS6 (planned, opt-in on Save As/Export).** For documents whose features need CS6 to
   understand them (pages, CS states). FirePixel's reader/writer (Apache-2.0) is the reference for
   that structure.

Fireworks CS5 opens FireCraft's MX-format files, including CS features stored the CS way
(9-slice symbol guides, per-object locks), and FireCraft opens CS5's files (it reads `MKBv`; see
"Fireworks CS files" above). Checked by hand in CS5.

## FireCraft extensions

FireCraft's own data, written as extra nodes or keys. Fireworks MX opens such files normally and
ignores what it doesn't know; whether the data survives Fireworks MX re-saving the file depends on
where it is (next section).

| Where | Node / key | Meaning | After a Fireworks MX re-save |
|---|---|---|---|
| root `MKBv` | `FCMi` | Colour math: `1` = high precision (double-precision blend modes and Hue/Saturation). Absent = Fireworks-exact. | lost (tested) |
| root `MKBv` | `FCPi` | `1` = perspective symbol instances use the pixel-correct warp. Absent = Fireworks MX's. | lost (tested) |
| `EXPv` | `FCFi` `FCQi` `FCAi` `FCLb` `FCJb` `FCMi` `FCOi` `FCXb` | Export: WebP/JPEG XL format, quality, alpha quality, lossless, full-resolution JPEG color, matte, PNG optimizer level, Smoother alpha ([behavior/export.md](behavior/export.md)) | lost (tested) |
| `TXTv` | `FAWi` | `1` = the text block grows with its text (made with a click, not a drag) | untested; probably lost (Fireworks MX rewrites text records) |
| `MSTv` | `FNGv` | 9-slice guides of a symbol that has a skewed or perspective instance, in place of `NGSv` (same `NGCf` values), because such an instance hangs Fireworks CS5. CS and MX ignore it; FireCraft reads either. | untested |
| effect `EPSv` | `firecraft_effect` key | A FireCraft effect stored as a Fireworks one (`grayscale`: Hue/Saturation at −100, [behavior/effects.md](behavior/effects.md)) | **kept** (tested), even when the effect is edited |

## Data that survives a Fireworks MX re-save (tested 2026-11-10)

**Why most extensions are lost.** On save, Fireworks MX writes the records it understands (the
document root, `EXPv`, text...) from its own internal structures, so fields it doesn't model are
dropped. An effect's parameters are the exception: an `EPSv` is an open dictionary of
`DCEv{DCKs key, DCVs value}` pairs (Fireworks effects are MOA plug-ins, and their parameters a
dictionary the host stores without understanding it). Fireworks MX loads the whole dictionary, lets
the effect's dialog change only the keys it knows, and writes the whole dictionary back.

**What was tested.** FireCraft wrote a carrier onto a bitmap (rainbow.png), each case its own file
opened in its own Fireworks MX session, listed through the scripting API, saved by Fireworks MX, and
read back with `dump_tree`. A dialog was caught by screenshot and dismissed by a click.

| Carrier | Opens | In Fireworks MX | Picture | Keys after re-save |
| --- | --- | --- | --- | --- |
| unknown `EffectMoaID`, without `mkbFile_WriteOnly_TemporaryEffectUiName`, on or off | yes, no dialog | dropped silently | unchanged | lost |
| unknown `EffectMoaID` **with** `mkbFile_WriteOnly_TemporaryEffectUiName` "FireCraft Data" (a plug-in-host ID `{d810e821-f86b-11d0-4669726543726166}` or any other, `{f12ec7af-...}`) | yes, with the missing-effects dialog | kept, listed as "FireCraft Data" | unchanged | kept, a 4096-character value byte for byte |
| a real plug-in effect (Splat LE Edges) whose plug-in isn't installed, plus extra keys | yes, with the missing-effects dialog | kept | unchanged | kept, extra keys too |
| Hue/Saturation (all 0), `EffectIsVisible false`, extra keys incl. a 4096-character value | yes, no dialog | kept, a disabled Hue/Saturation; scripts read the keys (`effect.fc_payload`) | unchanged | kept, byte for byte |
| the same on a hidden 1×1 transparent bitmap named "FireCraft data" | yes, no dialog | the object kept, hidden, named | unchanged | kept |

**Missing effects.** Fireworks MX keeps an effect it can't load (a plug-in that isn't installed:
removing EyeCandy4000.8bf and Splat LE-eng.8bf from a copy of the install kept all 45 effects of
plugin_test_windows.png and their 462 plug-in keys through a re-save) and says so on opening:
*"This document uses live effects or filters that are not currently installed. Objects using
missing effects are editable, but will not display the effects."*, listing them by their
`mkbFile_WriteOnly_TemporaryEffectUiName`. Without that key an effect it can't load is dropped
silently. Not a plug-in cache: deleting Fireworks.mch changed nothing. (FireCraft writes the key on
every effect it creates; effects it reads keep the record as it was.)

Earlier, with the Grayscale effect: Fireworks MX keeps the extra key when the user *edits* that
effect in Fireworks MX; it renames the effect ("Hue/Saturation...") and stores its new values
(minions_banana2_gray.png).

**Two recipes** (not used yet, apart from Grayscale):

1. *Named missing effect*: any `EffectMoaID` no Fireworks has (e.g. `{d810e821-f86b-11d0-4669726543726166}`,
   "FireCraf" in the plug-in host's format), `EffectIsVisible true`, `name` and
   `mkbFile_WriteOnly_TemporaryEffectUiName` "FireCraft Data", then FireCraft's keys. Clear to a
   user (it says what it is) but Fireworks MX shows the missing-effects dialog every time the file
   opens.
2. *Disabled neutral effect*: `EffectMoaID {3439b08d-1922-11d3-9bde00e02910d580}` (Hue/Saturation),
   `EffectIsVisible false`, `name Hue/Saturation...`, `category Adjust Color`,
   `MB_filter_preview_tile_size -1 -1`, `hue_amount 0`, `saturation_amount 0`, `lightness_amount 0`,
   `hls_colorize false`, then FireCraft's keys. No dialog, draws nothing even if enabled, but looks
   like an ordinary (disabled) Hue/Saturation.

Either one on an object carries object data (it travels with the object: duplicated, moved,
deleted with it); on a hidden 1×1 transparent bitmap, named so a user can tell what it is, it
carries document data.

**Design rules for using it** (from the tests and the Grayscale case):

* Users see it in Fireworks MX: the effect in the object's effect list (and, for recipe 1, the
  dialog on every open), the carrier object (hidden) in the Layers panel. They can delete it, move
  it or enable it. Write it only when there's FireCraft-only data to keep, never on every file.
* Fireworks MX keeps the keys even when the user changes what they describe. Store, next to the
  data, what it was written for (for document settings: the Fireworks MX values saved alongside,
  e.g. the `EXPv` format), and use the data only while that still holds; otherwise Fireworks MX's
  change wins.
* Prefix keys (`firecraft_...`), include a version key, keep values text (they're `DCVs` strings;
  4 KB tested).
* Reading back: FireCraft finds the carrier, applies its data, and doesn't show the carrier object
  or effect as ordinary content (it would rewrite them on save).

**Not tested yet:** copy and paste in Fireworks MX, symbols and groups, Fireworks CS5/CS6 (not
runnable here; CS may differ), values much larger than 4 KB, whether the missing-effects dialog can
be avoided for recipe 1.

**Re-running the test:** to simulate a missing plug-in, copy the Fireworks MX folder and move its
`.8bf` files out of the copy's `Plug-Ins`, then run that copy; the dialog needs a click on OK
(`tools/fwmx-oracle/xin.py click`) before a script continues. Build a file with the carrier (a short `firecraft-engine` example that opens
an image, pushes an `EffectEntry::from_pairs(...)` onto its objects and saves), open it in Fireworks
MX with a script per file (`tools/fwmx-oracle/fw_inspect.sh` pattern: `fw.openDocument`, read
`element.effectList.effects[k].<key>`, `fw.saveDocument`, then `fw.quitApplication`), and check the
re-saved file with `cargo run -p firecraft-fwpng --example dump_tree -- file.png | grep DCKs`.
