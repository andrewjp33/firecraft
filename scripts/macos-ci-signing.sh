#!/bin/bash
# Release workflow, macOS: make the Developer ID certificate and the notarization key available
# to scripts/build-macos.sh and scripts/package-release.sh, when the repository has them as
# secrets. Without them nothing happens and the app is signed ad hoc.
#
# Secrets (Settings > Secrets and variables > Actions), all optional:
#   MACOS_CERTIFICATE           the "Developer ID Application" certificate with its private key,
#                               exported from Keychain Access as .p12, base64-encoded
#                               (base64 -i cert.p12 | pbcopy)
#   MACOS_CERTIFICATE_PASSWORD  the .p12's password
#   MACOS_NOTARY_KEY            an App Store Connect API key (the AuthKey_XXXX.p8 file's text;
#                               Users and Access > Integrations > Team Keys, role Developer)
#   MACOS_NOTARY_KEY_ID         its key ID
#   MACOS_NOTARY_ISSUER         the issuer ID shown above the keys
# Writes MACOS_SIGN_IDENTITY and MACOS_NOTARY_KEY (a file path) to $GITHUB_ENV.
set -euo pipefail

if [ -z "${MACOS_CERTIFICATE:-}" ]; then
    echo "No MACOS_CERTIFICATE secret: the app is signed ad hoc (Gatekeeper asks once on first open)."
    exit 0
fi
keychain=$RUNNER_TEMP/firecraft-signing.keychain-db
keychain_password=$(openssl rand -base64 24)
cert=$RUNNER_TEMP/firecraft-cert.p12
base64 --decode <<<"$MACOS_CERTIFICATE" >"$cert"
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$cert" -k "$keychain" -P "${MACOS_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign
rm "$cert"
# Let codesign use the key without a password prompt.
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
identity=$(security find-identity -v -p codesigning "$keychain" | sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1)
[ -n "$identity" ] || { echo "MACOS_CERTIFICATE holds no Developer ID Application identity" >&2; exit 1; }
echo "Signing as: $identity"
echo "MACOS_SIGN_IDENTITY=$identity" >>"$GITHUB_ENV"

if [ -n "${MACOS_NOTARY_KEY:-}" ]; then
    key=$RUNNER_TEMP/firecraft-notary.p8
    printf '%s\n' "$MACOS_NOTARY_KEY" >"$key"
    echo "MACOS_NOTARY_KEY=$key" >>"$GITHUB_ENV"
    echo "Notarizing with API key ${MACOS_NOTARY_KEY_ID:-?}"
else
    echo "No MACOS_NOTARY_KEY secret: signed with Developer ID but not notarized."
fi
