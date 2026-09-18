#!/usr/bin/env bash
# Regenerates apps/desktop/src-tauri/resources/third-party-notices.html — the
# bundled "Open Source Licenses" page (Darkwave menu > Open Source Licenses),
# covering every Rust crate compiled into the binary and every JS package
# bundled into the frontend. See docs/adr/0034-third-party-notices.md.
#
# Run this whenever dependencies change (a `cargo add`/`npm install` that
# touches Cargo.lock or package-lock.json), and definitely before cutting a
# release — nothing regenerates this automatically today, so a stale copy
# won't error, it'll just quietly under-report what's actually shipping.
#
# Requires cargo-about (`cargo install cargo-about --locked --features cli`)
# and Python 3 (already required elsewhere in this repo's scripts).
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

OUT="apps/desktop/src-tauri/resources/third-party-notices.html"
mkdir -p "$(dirname "$OUT")"

RUST_HTML="$(mktemp)"
JS_HTML="$(mktemp)"
trap 'rm -f "$RUST_HTML" "$JS_HTML"' EXIT

echo "[1/3] Generating Rust license report (cargo about)..."
cargo about generate about.hbs -o "$RUST_HTML"

echo "[2/3] Generating JavaScript license report..."
python3 scripts/gen_js_notices.py > "$JS_HTML"

echo "[3/3] Merging into $OUT..."
python3 - "$RUST_HTML" "$JS_HTML" "$OUT" <<'PYEOF'
import sys

rust_path, js_path, out_path = sys.argv[1:4]
rust_html = open(rust_path, encoding="utf-8").read()
js_section = open(js_path, encoding="utf-8").read()

marker = "</ul>\n    </main>"
if marker not in rust_html:
    raise SystemExit(f"expected marker {marker!r} not found in cargo-about output — did about.hbs change shape?")

merged = rust_html.replace(marker, f"</ul>\n\n        {js_section}\n    </main>", 1)
with open(out_path, "w", encoding="utf-8") as f:
    f.write(merged)
PYEOF

echo "✓ $OUT regenerated."
