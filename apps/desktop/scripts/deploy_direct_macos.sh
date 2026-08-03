#!/usr/bin/env bash
# Direct-sale macOS build: build → DMG → sign → notarize → staple → distribute.
# Mirrors exposeu_wrapkit's scripts/production/deploy_direct_macos.sh, adapted
# to Darkwave's simpler dependency set (no bundled Qt/ffmpeg frameworks to
# sign individually — the app bundle itself is the only thing to sign).
#
# Run from apps/desktop: bash scripts/deploy_direct_macos.sh
#
# Prerequisites (one-time, interactive, not something this script can do):
#   1. A "Developer ID Application" certificate in your login keychain,
#      requested via Keychain Access > Certificate Assistant, uploaded to
#      developer.apple.com, downloaded and double-clicked to install.
#   2. A notarization keychain profile:
#        xcrun notarytool store-credentials darkwave-notary \
#          --apple-id <your Apple ID email> \
#          --team-id RD7UU4Z3D2 \
#          --password <an app-specific password from appleid.apple.com>
#
# One dependency to watch during notarization: `ort` (ONNX Runtime, used
# transitively for Silero VAD voice detection) links a dylib that must
# itself be properly signed and included in bundle.resources. If
# notarization or `codesign --verify --deep` flags it as an
# unsigned/unexpected library, that's the first place to look — not a
# preemptive disable-library-validation entitlement (Darkwave bundles no
# other non-Rust native frameworks needing that exception).

set -euo pipefail

# Pinned by SHA-1 fingerprint, not by name: the login keychain has two
# certs both named "Developer ID Application: ... (RD7UU4Z3D2)" — this one
# (2FDD...) is valid until 2031; the other (9A01...), issued far more
# recently, expires in just months (Apple caps cert validity to the
# Developer Program membership term, so that's likely when the membership
# itself is up for renewal). Matching by name would be ambiguous between
# the two, so pin the long-lived one explicitly.
IDENTITY="2FDD1878C9570F69A38D4231D9E6FC37E92D537F"
KEYCHAIN_PROFILE="darkwave-notary"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
# This is a Cargo workspace, so the build output lands under the workspace
# root's target/, not apps/desktop/src-tauri/target/ — a plain single-crate
# Tauri project (like exposeu_wrapkit) wouldn't have this distinction.
WORKSPACE_ROOT="$(cd "$PROJECT_ROOT/../.." && pwd)"
APP="$WORKSPACE_ROOT/target/release/bundle/macos/Darkwave.app"
DMG_DIR="$WORKSPACE_ROOT/target/release/bundle/dmg"
DMG_NAME="Darkwave_${VERSION}_aarch64.dmg"
DMG_OUT="$DMG_DIR/$DMG_NAME"

echo "▶  Darkwave v${VERSION} — Direct Distribution (macOS)"
echo "────────────────────────────────────────────────────"

echo "[1/5] Building (direct-dist, unsandboxed, Developer ID identity)..."
APPLE_SIGNING_IDENTITY="$IDENTITY" npx tauri build \
  --features direct-dist \
  --config src-tauri/tauri.direct.conf.json

echo "[2/5] Signing DMG..."
codesign --force --sign "$IDENTITY" --options runtime --timestamp "$DMG_OUT"

echo "[3/5] Notarizing (this takes a minute)..."
xcrun notarytool submit "$DMG_OUT" --keychain-profile "$KEYCHAIN_PROFILE" --wait

echo "[4/5] Stapling & validating..."
xcrun stapler staple "$DMG_OUT"
xcrun stapler validate "$DMG_OUT"
spctl -a -t open --context context:primary-signature "$DMG_OUT"

echo "[5/5] Done."
echo ""
echo "  → $DMG_OUT"
echo ""
echo "Manually verify the security-scoped bookmark / NAS reconnect behavior"
echo "does NOT apply here (this build is unsandboxed) before distributing —"
echo "see docs/development/release-readiness.md's manual checklist instead."
echo ""
echo "Next: copy to the licensing server's gated download mount and update"
echo "PRODUCTS.darkwave.downloadFiles.macos's target if the filename changed:"
echo "  licensing-server/releases/actual/Darkwave.dmg"
