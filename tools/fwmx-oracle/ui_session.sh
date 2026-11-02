#!/bin/bash
# Start FireCraft for UI tests on a virtual display with its accessibility tree available, so
# uimap.py can find widgets by name. Runs COMMANDS (a shell snippet) once the app is up, then
# saves a screenshot (with the cursor) and stops everything.
#
#   tools/fwmx-oracle/ui_session.sh OUTDIR FILE 'uimap.py click "Layers"; xin.py key Return'
#
# In COMMANDS, uimap.py and xin.py are on PATH-like shorthands ($UIMAP, $XIN).
set -u
OUT=$1; FILE=$2; CMDS=${3:-}
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../.." && pwd)
mkdir -p "$OUT"
exec dbus-run-session -- bash -c '
  set -u
  OUT="$1"; FILE="$2"; CMDS="$3"; HERE="$4"; ROOT="$5"
  D=:$((90 + RANDOM % 9))
  Xvfb $D -screen 0 1400x900x24 -nolisten tcp >/dev/null 2>&1 & XP=$!
  sleep 1
  export DISPLAY=$D
  /usr/libexec/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 & BP=$!
  sleep 1
  # Turn accessibility on (AccessKit only builds its tree for assistive technology).
  gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
    --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled "<true>" >/dev/null
  export XDG_DATA_HOME=$(mktemp -d) FIRECRAFT_NO_SPLASH=1 FIRECRAFT_RENDERER=glow LIBGL_ALWAYS_SOFTWARE=1
  export XCURSOR_THEME=${XCURSOR_THEME:-Yaru} XCURSOR_SIZE=${XCURSOR_SIZE:-24}
  "$ROOT/target/debug/firecraft" $FILE >"$OUT/app.log" 2>&1 & AP=$!
  sleep 7
  UIMAP="python3 $HERE/uimap.py"; XIN="python3 -I $HERE/xin.py"
  $XIN move 700 400 focus
  [ -n "$CMDS" ] && eval "$CMDS"
  sleep 1
  import -window root "$OUT/raw.png"
  python3 -I "$HERE/xcursor.py" "$OUT/raw.png" "$OUT/after.png" 2>/dev/null
  kill $AP $BP 2>/dev/null; sleep 0.5; kill $XP 2>/dev/null; wait 2>/dev/null
  true
' _ "$OUT" "$FILE" "$CMDS" "$HERE" "$ROOT"
