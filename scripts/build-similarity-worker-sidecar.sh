#!/usr/bin/env bash
# Builds crates/similarity-worker and copies it into
# apps/desktop/src-tauri/binaries/ under the platform-triple name Tauri's
# sidecar mechanism expects. Re-run this after any change to
# crates/similarity-worker, or on a new machine before `tauri dev`/`tauri build`.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
triple="$(rustc -vV | sed -n 's/^host: //p')"
suffix=""
if [[ "$triple" == *windows* ]]; then
  suffix=".exe"
fi

cargo build -p similarity-worker --release --manifest-path "$repo_root/Cargo.toml"

dest_dir="$repo_root/apps/desktop/src-tauri/binaries"
mkdir -p "$dest_dir"
cp "$repo_root/target/release/similarity-worker$suffix" \
   "$dest_dir/similarity-worker-$triple$suffix"

echo "Wrote $dest_dir/similarity-worker-$triple$suffix"
