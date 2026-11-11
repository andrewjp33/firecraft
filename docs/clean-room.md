# Clean room and the Fireworks MX oracle

FireCraft reimplements behaviour, not code. Sources, in order of preference:

1. **Documentation**: the *Using Fireworks MX* help (behaviour and UI descriptions).
2. **Measurement**: run the original under Wine and observe inputs → outputs
   (`tools/fwmx-oracle`). Results are written down as specifications in `docs/behavior/` and
   encoded as unit tests with the measured numbers.
3. **Static analysis, only when 1-2 can't answer a question**: used to find *what* to measure
   (e.g. which preferences and script functions drive the Magic Wand). Findings are recorded as
   behaviour in prose (with function addresses for reference); no code is copied or
   transliterated into FireCraft.
4. **File formats** are learned from files the original writes and verified by having it open
   files FireCraft writes (interoperability).
5. **Later versions** (Fireworks CS3-CS6 features such as 9-slice symbols and per-object locks):
   learned from files they write, a Fireworks CS5 executable examined (it isn't run
   here), and files checked by hand in Fireworks CS5 on Windows.

No Macromedia/Adobe artwork, icons, presets or sample files are committed. Screenshots of the
original for local reference go in `reference/` (git-ignored).

## Running the oracle

Requirements: Wine (tested 9.0), `xvfb-run`, ImageMagick `import`/`convert`, `libXtst`,
a Fireworks MX install folder and a Wine prefix where it is registered.

```sh
cp -a ~/.wine /tmp/fw-prefix            # always work on a copy: the harness edits preferences
export WINEPREFIX=/tmp/fw-prefix FWMX_DIR="$HOME/Documents/work/Fireworks MX"
tools/fwmx-oracle/fw_inspect.sh out/ file1.png file2.png    # does Fireworks open them? what's inside?
tools/fwmx-oracle/wand_edge.sh out/ disc.png aa32 true 0 32 # measure the wand on disc.png
```

Notes learned the hard way:

* Fireworks runs a `.jsf` passed on its command line **only as a Windows path**
  (`winepath -w`), and the path is handed to an already running instance via DDE.
* Some editing API calls (e.g. `selectSimilarFromPoint`) fail from a startup script with error
  `0x80041428` because no tool context exists yet; drive the UI with `xin.py` instead.
* Wine's built-in `comctl32` breaks Fireworks MX's panel docking (blank panel groups). Install the
  native one in the prefix: `winetricks -q comctl32`. GPU settings don't matter: Fireworks MX draws
  with GDI only.

## Testing FireCraft's UI

`tools/fwmx-oracle/ui_session.sh OUTDIR FILE 'COMMANDS'` starts FireCraft on a virtual display
with its accessibility tree available, runs the commands, and saves a screenshot. In the
commands, `$UIMAP click NAME` clicks a widget by its accessible name (the AccessKit tree read
over AT-SPI: menus, tools, Layers rows, buttons), `$UIMAP list FILTER` lists them, and `$XIN`
(xin.py) sends keys, clicks and drags (`key a`, `combo Control_L+z`, `move X Y down move X Y
up`). Clicking widgets by name avoids missed clicks from coordinates guessed off screenshots.

Copy/paste with Fireworks under Wine needs Wine's `explorer` running (as in a desktop session)
so Wine bridges its clipboard to X11; `xclip.py` reads and writes the X11 clipboard.
