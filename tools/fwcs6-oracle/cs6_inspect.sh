#!/bin/bash
# Open Fireworks PNGs in the real Fireworks CS6 (under Wine, on a private virtual display) and
# report what it sees.
#
#   CS6_PREFIX=~/Documents/work/CS6 tools/fwcs6-oracle/cs6_inspect.sh DIR a.png b.png
#
# Files are named relative to DIR. Writes DIR/log.txt (one line per document, one per layer, one
# per element with type, name, bounds, blend mode, opacity, visibility and effects) and re-saves each
# file as DIR/cs6_<name>: CS6's own encoding, whose composite is CS6's render of the document.
# "Missing Fonts" is answered with "No Change" automatically. Uses the prefix itself (CS6 updates
# its recent-files list there), so it refuses to run while CS6 is open in that prefix.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/cs6_lib.sh"
cs6_build_helper || exit 1
if cs6_running; then cs6_busy_message; exit 1; fi
O=$(cd "$1" && pwd); shift; U="file:///Z|$O/"; rm -f "$O/log.txt"
LIST=$(printf '"%s",' "$@"); LIST="[${LIST%,}]"
cat > "$O/inspect.jsf" <<EOF
var log = "";
function L(s) { log += s + "\n"; }
function get(o, k) { try { var v = o[k]; return v === undefined ? "-" : v; } catch (e) { return "?"; } }
var files = $LIST;
for (var i = 0; i < files.length; i++) {
  try {
    fw.openDocument("$U" + files[i], false);
    var dom = fw.getDocumentDOM();
    L(files[i] + ": " + dom.width + "x" + dom.height + " pages=" + get(dom, "pagesCount") + " frames=" + dom.frames.length + " layers=" + dom.layers.length + " bg=" + dom.backgroundColor);
    for (var l = 0; l < dom.layers.length; l++) {
      var lay = dom.layers[l]; var els = lay.frames[0].elements;
      L("  layer " + l + " '" + lay.name + "' type=" + lay.layerType + " els=" + els.length);
      for (var e = 0; e < els.length; e++) {
        var el = els[e]; var fx = "";
        try { var ef = el.effectList ? el.effectList.effects : [];
              for (var k = 0; k < ef.length; k++) fx += " " + ef[k].EffectMoaID + (ef[k].EffectIsVisible ? "" : "(off)"); } catch (x) { fx = " ?"; }
        L("    " + el + " '" + get(el, "name") + "' " + el.left + "," + el.top + " " + el.width + "x" + el.height +
          " blend=" + get(el, "blendMode") + " opacity=" + get(el, "opacity") + " visible=" + get(el, "visible") + " fx=[" + fx + " ]");
      }
    }
    fw.saveDocument(dom, "${U}cs6_" + files[i]);
    fw.closeDocument(dom, false);
  } catch (e) { L(files[i] + " ERR " + e); }
}
Files.createFile("${U}log.txt", ".txt", "FWMX");
var f = Files.open("${U}log.txt", true); f.write(log); f.close();
fw.quitApplication();
EOF
cd "$CS6_DIR"
# Scripts must be passed as Windows paths (backslashes) or Fireworks ignores them.
JSF=$(winepath -w "$O/inspect.jsf")
xvfb-run -a -s "-screen 0 1600x1000x24" bash -c '
  wine Fireworks.exe "$1" & P=$!
  wine "$2" nofonts & H=$!
  for i in $(seq 1 90); do sleep 2; [ -f "$3/log.txt" ] && break; done
  sleep 2; kill $P $H 2>/dev/null; wineserver -k; true' _ "$JSF" "$CS6_HELPER" "$O" >/dev/null 2>&1
if [ -f "$O/log.txt" ]; then cat "$O/log.txt"; else echo "no log: CS6 did not finish (stuck on a dialog?)" >&2; exit 1; fi
