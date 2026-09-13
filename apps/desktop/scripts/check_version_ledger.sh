#!/usr/bin/env bash
# Shared preflight check, sourced by mac_sign_and_package_mas.sh and
# deploy_direct_macos.sh (and mirrored in check_version_ledger.ps1 for
# deploy_direct_windows.ps1) — reads docs/macos/version-ledger.md itself
# instead of trusting a human to remember to check it first.
#
# Exists because the exact same App Store Connect 409 ("must contain a
# higher version than that of the previously approved version") happened
# twice from the same root cause: a ledger row sat at "Uploaded" after it
# had actually been approved, and the next build reused that version since
# nothing caught it before spending a full build+sign cycle. This can't
# know an approval happened before a human updates the ledger's Status
# field, but it CAN stop a build from repeating a version/build the ledger
# already shows is unsafe, the moment that's checked rather than after
# Transporter rejects it.
#
# Usage: source this file, then call:
#   check_version_ledger "$CURRENT_VERSION" "$CURRENT_BUILD"
# Pass "" for CURRENT_BUILD to skip the build-number check (the direct-sale
# channel has no CFBundleVersion-uniqueness requirement — see the ledger's
# own "direct-dist / Store Manager" section).

_version_ledger_path() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  # apps/desktop/scripts -> apps/desktop -> workspace root
  echo "$(cd "$script_dir/../.." && pwd)/docs/macos/version-ledger.md"
}

# Echoes -1, 0, or 1 for $1 <, ==, > $2 (dotted numeric versions only).
_semver_cmp() {
  local a="$1" b="$2"
  local -a av bv
  IFS='.' read -r -a av <<< "$a"
  IFS='.' read -r -a bv <<< "$b"
  local i
  for i in 0 1 2; do
    local ai="${av[$i]:-0}" bi="${bv[$i]:-0}"
    if (( 10#$ai < 10#$bi )); then echo -1; return; fi
    if (( 10#$ai > 10#$bi )); then echo 1; return; fi
  done
  echo 0
}

check_version_ledger() {
  local current_version="$1"
  local current_build="${2:-}"
  local ledger
  ledger="$(_version_ledger_path)"

  if [ ! -f "$ledger" ]; then
    echo "warn: $ledger not found — skipping version-ledger preflight check" >&2
    return 0
  fi

  local approved_version="" max_build=0
  local date version build status rest
  while IFS='|' read -r _ date version build status rest; do
    date="$(echo "$date" | xargs)"
    version="$(echo "$version" | xargs)"
    build="$(echo "$build" | xargs)"
    status="$(echo "$status" | xargs)"
    [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || continue
    if [ -z "$approved_version" ] && [[ "$status" == Approved* ]]; then
      approved_version="$version"
    fi
    if [[ "$build" =~ ^[0-9]+$ ]] && (( build > max_build )); then
      max_build="$build"
    fi
  done < "$ledger"

  if [ -n "$approved_version" ] && [ "$approved_version" != "—" ]; then
    local cmp
    cmp="$(_semver_cmp "$current_version" "$approved_version")"
    if [ "$cmp" -le 0 ]; then
      cat >&2 <<EOF

✗ version-ledger check failed
  Current marketing version ($current_version) does not exceed the last
  APPROVED version recorded in docs/macos/version-ledger.md ($approved_version).
  App Store Connect (and, for consistency, the direct-sale/Windows builds
  that share this same version) will reject or duplicate this exact
  version. Bump the version in tauri.conf.json, package.json, and
  Cargo.toml (all three — see the ledger's "must all move together" note)
  before building.

EOF
      exit 1
    fi
  fi

  if [ -n "$current_build" ]; then
    if [ "$current_build" -le "$max_build" ]; then
      cat >&2 <<EOF

✗ version-ledger check failed
  Current build number ($current_build) is not higher than the highest
  build number ever recorded in docs/macos/version-ledger.md ($max_build).
  CFBundleVersion must simply be unused, full stop, regardless of whether
  the marketing version changed. Bump CFBundleVersion in
  apps/desktop/src-tauri/Info.plist before building.

EOF
      exit 1
    fi
    echo "✓ version-ledger check passed — building $current_version/$current_build (last approved: ${approved_version:-none}, highest build used: $max_build)"
  else
    echo "✓ version-ledger check passed — building $current_version (last approved: ${approved_version:-none})"
  fi
}
