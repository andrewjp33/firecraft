#!/bin/bash
# Open Fireworks PNGs in the real Fireworks MX (under Wine, headless) and report what it sees.
#
#   FWMX_DIR=/path/to/"Fireworks MX" WINEPREFIX=/path/to/prefix tools/fwmx-oracle/fw_inspect.sh DIR a.png b.png
#
# Writes DIR/log.txt (one line per document plus one per layer with its elements) and re-saves each
# file as DIR/fw_<name> (Fireworks' own re-encoding, useful for diffs). Uses a copy of a Wine prefix
# in which Fireworks MX is registered; never point it at your everyday prefix.
set -u
: "${FWMX_DIR:?set FWMX_DIR to the Fireworks MX install folder}"
: "${WINEPREFIX:?set WINEPREFIX to a scratch copy of a prefix with Fireworks MX registered}"
export WINEDEBUG=-all
O=$(cd "$1" && pwd); shift; U="file:///Z|$O/"; rm -f "$O/log.txt"
LIST=$(printf '"%s",' "$@"); LIST="[${LIST%,}]"
cat > "$O/inspect.jsf" <<EOF
var log = "";
function L(s) { log += s + "\n"; }
var files = $LIST;
for (var i = 0; i < files.length; i++) {
  try {
    fw.openDocument("$U" + files[i], false);
    var dom = fw.getDocumentDOM();
    L(files[i] + ": " + dom.width + "x" + dom.height + " frames=" + dom.frames.length + " layers=" + dom.layers.length + " bg=" + dom.backgroundColor);
    for (var l = 0; l < dom.layers.length; l++) {
      var lay = dom.layers[l]; var els = lay.frames[0].elements;
      var s = "  layer " + l + " '" + lay.name + "' type=" + lay.layerType + " els=" + els.length + ":";
      for (var e = 0; e < els.length; e++) { var el = els[e]; s += " [" + el + " " + el.left + "," + el.top + " " + el.width + "x" + el.height + "]"; }
      L(s);
    }
    fw.saveDocument(dom, "${U}fw_" + files[i]);
    fw.closeDocument(dom, false);
  } catch (e) { L(files[i] + " ERR " + e); }
}
Files.createFile("${U}log.txt", ".txt", "FWMX");
var f = Files.open("${U}log.txt", true); f.write(log); f.close();
fw.quitApplication();
EOF
cd "$FWMX_DIR"
# Scripts must be passed as Windows paths (backslashes) or Fireworks ignores them.
xvfb-run -a -s "-screen 0 1280x1024x24" bash -c "
wine Fireworks.exe \"\$(winepath -w '$O/inspect.jsf')\" 2>/dev/null & P=\$!
for i in \$(seq 1 60); do sleep 2; [ -f '$O/log.txt' ] && break; done
kill \$P 2>/dev/null; wineserver -k; true" >/dev/null 2>&1
cat "$O/log.txt"
