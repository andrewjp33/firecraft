#!/bin/bash
# Package a release build for download: the program plus the README, licences and attribution
# (the licence notices must ship with the binaries, see ATTRIBUTION.md).
#
# Usage: scripts/package-release.sh linux|windows|macos|web VERSION [OUT_DIR]
# Reads the builds from dist/ (`make release`, `make windows-release`, `make macos-release`,
# `make web`; DIST overrides the folder) and writes to OUT_DIR (default dist/packages):
#   firecraft-VERSION-linux-x86_64.tar.gz    firecraft
#   firecraft-VERSION-windows-x86_64.zip     firecraft.exe
#   firecraft-VERSION-macos-universal.dmg    FireCraft.app (Apple silicon and Intel), on a Mac
#   firecraft-VERSION-web.zip                the browser version, to serve from any web server
#
# macOS: with MACOS_SIGN_IDENTITY set (see scripts/build-macos.sh) the disk image is signed too,
# and with App Store Connect API key credentials it's notarized by Apple and the ticket stapled
# to it, so it opens without Gatekeeper's warning: MACOS_NOTARY_KEY (path of the AuthKey_*.p8
# file), MACOS_NOTARY_KEY_ID and MACOS_NOTARY_ISSUER; or MACOS_NOTARY_PROFILE, a keychain
# profile saved with `xcrun notarytool store-credentials`.
set -euo pipefail
cd "$(dirname "$0")/.."
kind=${1:?linux, windows, macos or web}
version=${2:?version, e.g. 0.1.0}
dist=${DIST:-dist}
out=${3:-$dist/packages}
mkdir -p "$out"
out=$(cd "$out" && pwd) # an absolute path (macOS's realpath has no -m)

case $kind in
    linux) name=firecraft-$version-linux-x86_64; files=("$dist/linux-release/firecraft") ;;
    windows) name=firecraft-$version-windows-x86_64; files=("$dist/windows-release/firecraft.exe") ;;
    macos) name=firecraft-$version-macos-universal; files=("$dist/macos-release/FireCraft.app") ;;
    web) name=firecraft-$version-web; files=("$dist/web/"*) ;;
    *) echo "unknown kind: $kind" >&2; exit 1 ;;
esac
for f in "${files[@]}"; do
    [ -e "$f" ] || { echo "missing build: $f" >&2; exit 1; }
done

stage=$(mktemp -d)
trap 'rm -r "$stage"' EXIT
mkdir "$stage/$name"
if [ "$kind" = macos ]; then
    # ditto keeps the bundle exactly as signed (cp could drop extended attributes).
    ditto "${files[0]}" "$stage/$name/FireCraft.app"
    # Drag FireCraft onto Applications to install it.
    ln -s /Applications "$stage/$name/Applications"
else
    cp -r "${files[@]}" "$stage/$name/"
fi
cp README.md LICENSE-MIT LICENSE-APACHE ATTRIBUTION.md "$stage/$name/"

case $kind in
    linux)
        chmod +x "$stage/$name/firecraft"
        tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
        echo "$out/$name.tar.gz" ;;
    macos)
        dmg=$out/$name.dmg
        if [ -e "$dmg" ]; then rm "$dmg"; fi
        hdiutil create -quiet -volname "FireCraft $version" -srcfolder "$stage/$name" -fs HFS+ \
            -format UDZO -imagekey zlib-level=9 "$dmg"
        if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
            codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$dmg"
        fi
        notary=()
        if [ -n "${MACOS_NOTARY_KEY:-}" ]; then
            notary=(--key "$MACOS_NOTARY_KEY" --key-id "${MACOS_NOTARY_KEY_ID:?}" --issuer "${MACOS_NOTARY_ISSUER:?}")
        elif [ -n "${MACOS_NOTARY_PROFILE:-}" ]; then
            notary=(--keychain-profile "$MACOS_NOTARY_PROFILE")
        fi
        if [ ${#notary[@]} -gt 0 ]; then
            [ -n "${MACOS_SIGN_IDENTITY:-}" ] || { echo "notarizing needs a Developer ID signature (MACOS_SIGN_IDENTITY)" >&2; exit 1; }
            echo "Notarizing $dmg (usually a few minutes)" >&2
            result=$(xcrun notarytool submit "$dmg" "${notary[@]}" --wait --timeout 30m --output-format json)
            status=$(plutil -extract status raw -o - - <<<"$result" 2>/dev/null || echo unknown)
            if [ "$status" != Accepted ]; then
                echo "Notarization: $status" >&2
                id=$(plutil -extract id raw -o - - <<<"$result" 2>/dev/null || true)
                # Apple's log says which file and why.
                if [ -n "$id" ]; then xcrun notarytool log "$id" "${notary[@]}" >&2 || true; fi
                exit 1
            fi
            xcrun stapler staple "$dmg"
        fi
        echo "$dmg" ;;
    *)
        # zip, or 7-Zip on the Windows build machines (Git Bash has no zip).
        if command -v zip >/dev/null; then (cd "$stage" && zip -qr9 "$out/$name.zip" "$name")
        else (cd "$stage" && 7z a -tzip -mx=9 -bd "$out/$name.zip" "$name" >/dev/null); fi
        echo "$out/$name.zip" ;;
esac
