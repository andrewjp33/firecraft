#!/bin/bash
# Start Fireworks CS6 under Wine with the mouse-drag fix (see cs6helper.c), on $DISPLAY.
#
#   CS6_PREFIX=~/Documents/work/CS6 tools/fwcs6-oracle/cs6_launch.sh [file.png ...]
#
# Returns once Fireworks exits. Files are passed as Windows paths (Fireworks also takes .jsf
# scripts this way). Set CS6_NOFONTS=1 to answer "Missing Fonts" with "No Change" automatically.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/cs6_lib.sh"
cs6_build_helper || exit 1
if cs6_running; then cs6_busy_message; exit 1; fi
ARGS=()
for f in "$@"; do ARGS+=("$(winepath -w "$(realpath "$f")")"); done
cd "$CS6_DIR"
wine Fireworks.exe "${ARGS[@]}" &
FW=$!
wine "$CS6_HELPER" ${CS6_NOFONTS:+nofonts} &
wait $FW
wait
