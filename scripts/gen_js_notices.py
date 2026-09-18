#!/usr/bin/env python3
"""Generates the "JavaScript (bundled into the frontend)" section of
apps/desktop's third-party notices page — the JS-side equivalent of what
`cargo about` already does for Rust crates (see
scripts/generate-third-party-notices.sh, which calls this).

There's no cargo-about equivalent wired up for npm here (license-checker
et al. are real options, but this repo's own preference — see
docs/adr/0025 and friends — is small hand-written logic over a new
dependency when the actual problem is this bounded: `npm run check`'s own
production dependency tree is 12-13 packages, not hundreds). This walks
the resolved production dependency tree via `npm ls --all --omit=dev
--json` (skips devDependencies entirely — those never ship, so they don't
belong in a notices file for the shipped app), reads each package's own
license field and bundled LICENSE file, and renders the same
overview/license-list HTML structure and CSS classes `about.hbs` already
defines, so both sections render identically without needing their own
separate stylesheet.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

DESKTOP_DIR = Path(__file__).resolve().parent.parent / "apps" / "desktop"
# This app's own packages — not a third-party notice.
FIRST_PARTY = {"@darkwave/design-tokens", "@darkwave/desktop"}
LICENSE_FILE_NAMES = ["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENSE-MIT", "LICENSE_MIT"]

# @tauri-apps/plugin-{clipboard-manager,dialog,opener} publish only a
# LICENSE.spdx *metadata* stub (not license text) alongside `"license":
# "MIT OR Apache-2.0"` — but @tauri-apps/api, from the same publisher
# ("Tauri Apps Contributors"), bundles the real text and is always a
# co-installed sibling here. Confirmed byte-identical against the actual
# upstream file (github.com/tauri-apps/plugins-workspace's own LICENSE_MIT,
# fetched directly) before relying on it as a stand-in, rather than
# assuming a same-org project necessarily shares text verbatim.
TAURI_PLUGIN_WORKSPACE_PACKAGES = {
    "@tauri-apps/plugin-clipboard-manager",
    "@tauri-apps/plugin-dialog",
    "@tauri-apps/plugin-opener",
}


def resolved_production_packages() -> dict[str, str]:
    """{name: version} for every package actually in the shipped bundle —
    walks the full resolved tree, not just direct dependencies, since a
    transitive package (e.g. motion-dom, pulled in by motion) still ships
    inside the built app just as much as a direct one does."""
    result = subprocess.run(
        ["npm", "ls", "--all", "--omit=dev", "--json"],
        cwd=DESKTOP_DIR,
        capture_output=True,
        text=True,
        check=False,
    )
    data = json.loads(result.stdout or "{}")
    packages: dict[str, str] = {}

    def walk(deps):
        if not deps:
            return
        for name, info in deps.items():
            version = info.get("version")
            if version and name not in packages:
                packages[name] = version
                walk(info.get("dependencies"))

    walk(data.get("dependencies"))
    for name in FIRST_PARTY:
        packages.pop(name, None)
    return packages


def find_license_text(name: str, pkg_dir: Path, node_modules: Path) -> str | None:
    for filename in LICENSE_FILE_NAMES:
        candidate = pkg_dir / filename
        if candidate.is_file():
            return candidate.read_text(encoding="utf-8", errors="replace")
    if name in TAURI_PLUGIN_WORKSPACE_PACKAGES:
        sibling = node_modules / "@tauri-apps" / "api" / "LICENSE_MIT"
        if sibling.is_file():
            return sibling.read_text(encoding="utf-8", errors="replace")
    return None


def html_escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def main() -> int:
    packages = resolved_production_packages()
    if not packages:
        print("gen_js_notices: resolved zero production packages — refusing to write an empty section", file=sys.stderr)
        return 1

    node_modules = DESKTOP_DIR.parent.parent / "node_modules"
    # groups: license_text -> {"license_field": str, "used_by": [(name, version)]}
    groups: dict[str, dict] = {}
    missing_license_text = []

    for name, version in sorted(packages.items()):
        pkg_dir = node_modules / name
        pkg_json_path = pkg_dir / "package.json"
        license_field = "UNKNOWN"
        if pkg_json_path.is_file():
            license_field = json.loads(pkg_json_path.read_text()).get("license", "UNKNOWN")

        text = find_license_text(name, pkg_dir, node_modules)
        if text is None:
            missing_license_text.append(f"{name}@{version} (license field: {license_field})")
            continue

        key = text.strip()
        group = groups.setdefault(key, {"license_field": license_field, "used_by": []})
        group["used_by"].append((name, version))

    if missing_license_text:
        print(
            "gen_js_notices: no bundled LICENSE file found for: " + ", ".join(missing_license_text)
            + " — add their text to this script's LICENSE_FILE_NAMES handling or a manual override before shipping",
            file=sys.stderr,
        )
        return 1

    overview_items = []
    license_items = []
    for index, (text, group) in enumerate(sorted(groups.items(), key=lambda kv: kv[1]["license_field"])):
        anchor = f"js-license-{index}"
        label = html_escape(group["license_field"])
        overview_items.append(f'<li><a href="#{anchor}">{label}</a> ({len(group["used_by"])})</li>')
        used_by = "\n".join(
            f'<li>{html_escape(name)} {html_escape(version)}</li>' for name, version in group["used_by"]
        )
        license_items.append(
            f'<li class="license">\n'
            f'    <h4 id="{anchor}">{label}</h4>\n'
            f'    <h5>Used by:</h5>\n'
            f'    <ul class="license-used-by">\n{used_by}\n    </ul>\n'
            f'    <pre class="license-text">{html_escape(text)}</pre>\n'
            f'</li>'
        )

    section = (
        '<h2>JavaScript (bundled into the frontend)</h2>\n'
        '<h3>Overview of licenses:</h3>\n'
        '<ul class="licenses-overview">\n' + "\n".join(overview_items) + "\n</ul>\n"
        '<h3>All license text:</h3>\n'
        '<ul class="licenses-list" id="js-licenses-list">\n' + "\n".join(license_items) + "\n</ul>\n"
    )
    print(section)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
