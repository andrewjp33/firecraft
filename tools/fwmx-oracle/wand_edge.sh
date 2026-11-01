#!/bin/bash
# Measure Fireworks MX's Magic Wand by driving its UI: open IMAGE, pick the wand, click at the
# image centre, press Delete, save. Wand settings are written into the (scratch) prefix's prefs.
#
#   FWMX_DIR=... WINEPREFIX=... tools/fwmx-oracle/wand_edge.sh OUT_DIR IMAGE.png NAME AA(true|false) FEATHER TOLERANCE
#
# Produces OUT_DIR/out_NAME.png (a Fireworks PNG whose flattened image shows the result). The
# window position is found from a screenshot, so it tolerates Wine placing the window differently.
set -u
: "${FWMX_DIR:?}"; : "${WINEPREFIX:?}"; export WINEDEBUG=-all
HERE=$(cd "$(dirname "$0")" && pwd)
O=$(cd "$1" && pwd); IMG=$(cd "$(dirname "$2")" && pwd)/$(basename "$2"); NAME=$3; AA=$4; FE=$5; TOL=$6
U="file:///Z|$O/"
PREFS="$(ls -d "$WINEPREFIX"/drive_c/users/*/AppData/Roaming/Macromedia/'Fireworks MX')/Fireworks MX Preferences.txt"
python3 -I - "$PREFS" "$AA" "$FE" "$TOL" <<'PY'
import sys, re
p, aa, fe, tol = sys.argv[1:]
s = open(p, encoding='latin1').read()
s = re.sub(r'(<key>WandToolAntiAliased</key>\s*)<bool>\w+</bool>', r'\1<bool>' + aa + '</bool>', s)
s = re.sub(r'(<key>WandToolFeatherAmount</key>\s*)<integer>\d+</integer>', r'\g<1><integer>' + fe + '</integer>', s)
s = re.sub(r'(<key>WandToolTolerance</key>\s*)<integer>\d+</integer>', r'\g<1><integer>' + tol + '</integer>', s)
open(p, 'w', encoding='latin1').write(s)
PY
echo "fw.openDocument(\"file:///Z|$IMG\", false);" > "$O/open.jsf"
cat > "$O/save_$NAME.jsf" <<J
fw.saveDocument(fw.getDocumentDOM(), "${U}out_$NAME.png");
Files.createFile("${U}done_$NAME.txt", ".txt", "FWMX");
J
cd "$FWMX_DIR"
xvfb-run -a -s "-screen 0 1280x1024x24" bash -c "
wine Fireworks.exe \"\$(winepath -w '$O/open.jsf')\" 2>/dev/null & P=\$!
sleep 20
import -window root '$O/shot.png'
TOP=\$(python3 -I - '$O/shot.png' <<'PY'
import sys, subprocess
raw = subprocess.run(['convert', sys.argv[1], '-crop', '1x1024+500+0', '-depth', '8', 'rgb:-'], capture_output=True).stdout
print(next((y for y in range(len(raw) // 3) if sum(raw[3*y:3*y+3]) > 30), 0))
PY
)
# Tools panel offsets in Fireworks MX's default layout: wand at (19, top+158); 100x100 image centre at (189, top+148).
python3 -I '$HERE/xin.py' click 19 \$((TOP+158)) sleep 1 click 189 \$((TOP+148)) sleep 1.5 key Delete sleep 1
wine Fireworks.exe \"\$(winepath -w '$O/save_$NAME.jsf')\" 2>/dev/null &
for k in \$(seq 1 15); do sleep 2; [ -f '$O/done_$NAME.txt' ] && break; done
kill \$P 2>/dev/null; wineserver -k; true" >/dev/null 2>&1
ls -la "$O/out_$NAME.png"
