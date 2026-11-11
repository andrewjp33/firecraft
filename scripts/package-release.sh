#!/bin/bash
# Package a release build for download: the program plus the README, licences and attribution
# (the licence notices must ship with the binaries, see ATTRIBUTION.md).
#
# Usage: scripts/package-release.sh linux|windows|web VERSION [OUT_DIR]
# Reads the builds from dist/ (`make release`, `make windows-release`, `make web`; DIST
# overrides the folder) and writes to OUT_DIR (default dist/packages):
#   firecraft-VERSION-linux-x86_64.tar.gz    firecraft
#   firecraft-VERSION-windows-x86_64.zip     firecraft.exe
#   firecraft-VERSION-web.zip                the browser version, to serve from any web server
set -euo pipefail
cd "$(dirname "$0")/.."
kind=${1:?linux, windows or web}
version=${2:?version, e.g. 0.1.0}
dist=${DIST:-dist}
out=$(realpath -m "${3:-$dist/packages}")
mkdir -p "$out"

case $kind in
    linux) name=firecraft-$version-linux-x86_64; files=("$dist/linux-release/firecraft") ;;
    windows) name=firecraft-$version-windows-x86_64; files=("$dist/windows-release/firecraft.exe") ;;
    web) name=firecraft-$version-web; files=("$dist/web/"*) ;;
    *) echo "unknown kind: $kind" >&2; exit 1 ;;
esac
for f in "${files[@]}"; do
    [ -e "$f" ] || { echo "missing build: $f" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -r "$stage"' EXIT
mkdir "$stage/$name"
cp -r "${files[@]}" "$stage/$name/"
cp README.md LICENSE-MIT LICENSE-APACHE ATTRIBUTION.md "$stage/$name/"

case $kind in
    linux)
        chmod +x "$stage/$name/firecraft"
        tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
        echo "$out/$name.tar.gz" ;;
    *)
        (cd "$stage" && zip -qr9 "$out/$name.zip" "$name")
        echo "$out/$name.zip" ;;
esac
