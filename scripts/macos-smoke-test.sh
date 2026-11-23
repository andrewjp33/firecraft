#!/bin/bash
# Check a built FireCraft.app on a Mac: the signature is valid, both architectures are in it,
# and each starts and keeps running (the Intel one under Rosetta on an Apple silicon Mac).
# The release workflow runs it after the macOS build; screenshots go to OUT_DIR.
#
# Usage: scripts/macos-smoke-test.sh [APP] [OUT_DIR]
#   APP      default dist/macos-release/FireCraft.app
#   OUT_DIR  default target/macos-smoke-test
set -uo pipefail
cd "$(dirname "$0")/.."
app=${1:-dist/macos-release/FireCraft.app}
out=${2:-target/macos-smoke-test}
bin=$app/Contents/MacOS/firecraft
mkdir -p "$out"
failed=0

codesign --verify --deep --strict --verbose=2 "$app" || failed=1
codesign --display --verbose=2 "$app" 2>&1 | grep -E '^(Identifier|Format|Authority|Signature|TeamIdentifier|Runtime)' || true
# Gatekeeper accepts only Developer ID signatures that Apple has notarized.
spctl --assess --type execute --verbose=2 "$app" 2>&1 ||
    echo "(Gatekeeper would ask before the first open: expected for an ad hoc signature)"
archs=$(lipo -archs "$bin")
echo "Architectures: $archs"

for arch in $archs; do
    if ! arch "-$arch" /usr/bin/true 2>/dev/null; then
        echo "$arch: can't run on this Mac (no Rosetta?), skipped"
        continue
    fi
    log=$out/$arch.log
    # No terminal here, so FireCraft also logs to ~/Library/Logs/FireCraft.log.
    arch "-$arch" "$bin" >"$log" 2>&1 &
    pid=$!
    sleep 25
    if kill -0 "$pid" 2>/dev/null; then
        echo "$arch: running after 25 s"
        screencapture -x "$out/$arch.png" 2>/dev/null || echo "(no screenshot)"
        kill "$pid"
        wait "$pid" 2>/dev/null
    else
        wait "$pid"
        echo "$arch: exited early (status $?)" >&2
        failed=1
    fi
    cat "$log"
    [ -f ~/Library/Logs/FireCraft.log ] && cat ~/Library/Logs/FireCraft.log
done
exit $failed
