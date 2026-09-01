#!/usr/bin/env bash
# Mac App Store build: build (sandboxed, no similarity-worker sidecar per
# the still-open GPL question — see docs/development/release-readiness.md)
# → sign → package .pkg → ready for App Store Connect upload.
# Mirrors exposeu_wrapkit's scripts/production/mac_sign_and_package.sh,
# simplified: no bundled frameworks/dylibs/extra sidecars to sign
# individually (see apps/desktop/src-tauri/Cargo.toml — no Qt/ffmpeg).
#
# Run from apps/desktop: bash scripts/mac_sign_and_package_mas.sh
#
# Prerequisites (one-time, interactive):
#   1. "3rd Party Mac Developer Application" + "3rd Party Mac Developer
#      Installer" certificates in your login keychain (Certificate
#      Assistant > request > upload to developer.apple.com > download >
#      install), issued under Team RD7UU4Z3D2.
#   2. A provisioning profile for dev.darkwave.app, downloaded from
#      developer.apple.com and saved as src-tauri/embedded.provisionprofile
#      (requires the bundle id to be registered first — see Phase I).

set -euo pipefail

APP_IDENTITY="3rd Party Mac Developer Application: Nudson Alan Terrinha Alves (RD7UU4Z3D2)"
INSTALLER_IDENTITY="3rd Party Mac Developer Installer: Nudson Alan Terrinha Alves (RD7UU4Z3D2)"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# This is a Cargo workspace, so the build output lands under the workspace
# root's target/, not apps/desktop/src-tauri/target/.
WORKSPACE_ROOT="$(cd "$PROJECT_ROOT/../.." && pwd)"
APP_PATH="$WORKSPACE_ROOT/target/release/bundle/macos/Darkwave.app"
PROVISION="src-tauri/embedded.provisionprofile"
APP_ENTITLEMENTS="src-tauri/entitlements.mas.plist"

echo "▶  Darkwave — Mac App Store package"
echo "────────────────────────────────────"

echo "[1/6] Building (sandboxed, MAS identity, no similarity-worker sidecar)..."
APPLE_SIGNING_IDENTITY="$APP_IDENTITY" npx tauri build \
  --config src-tauri/tauri.mas.conf.json

echo "[2/6] Removing quarantine attributes..."
xattr -rc "$APP_PATH"

echo "[3/6] Embedding provisioning profile..."
if [ ! -f "$PROVISION" ]; then
  echo "Missing $PROVISION — download it from developer.apple.com first" \
       "(requires dev.darkwave.app to be registered, see Phase I)." >&2
  exit 1
fi
cp "$PROVISION" "$APP_PATH/Contents/embedded.provisionprofile"
xattr -rc "$APP_PATH"

echo "[4/6] Signing app bundle..."
codesign --force --verify --verbose \
  --sign "$APP_IDENTITY" \
  --entitlements "$APP_ENTITLEMENTS" \
  --options runtime \
  "$APP_PATH"
xattr -rc "$APP_PATH"

echo "[5/6] Verifying signature..."
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

echo "[6/6] Building .pkg for App Store Connect..."
mkdir -p builds
# Version-stamped filename so a stale package can't be uploaded by mistake —
# App Store Connect rejects (409) any upload whose CFBundleShortVersionString
# isn't higher than the last APPROVED version, and a generic name makes that
# easy to trip. Read both keys straight out of the app that was just signed.
SHORT_VER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP_PATH/Contents/Info.plist")
BUILD_VER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "$APP_PATH/Contents/Info.plist")
PKG_OUT="builds/Darkwave_${SHORT_VER}_b${BUILD_VER}.pkg"
rm -f builds/*.pkg
COPYFILE_DISABLE=1 productbuild \
  --component "$APP_PATH" /Applications \
  --sign "$INSTALLER_IDENTITY" \
  "$PKG_OUT"

echo ""
echo "✓ $PKG_OUT ready  (version $SHORT_VER, build $BUILD_VER)"
echo ""
echo "Upload with Transporter (add this exact file — don't retry a prior delivery) or:"
echo "  xcrun altool --validate-app -f $PKG_OUT -t macos --apple-id alan.creative@icloud.com --password <app-specific-pw> --team-id RD7UU4Z3D2"
echo "  xcrun altool --upload-app   -f $PKG_OUT -t macos --apple-id alan.creative@icloud.com --password <app-specific-pw> --team-id RD7UU4Z3D2"
