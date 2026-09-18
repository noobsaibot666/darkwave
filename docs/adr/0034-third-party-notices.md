# ADR 0034: Bundle a real third-party notices page

## Status

Accepted.

## Context

Auditing this app's license exposure (ADR 0033, then a follow-up check of
Silero VAD's own model license) turned up a related, smaller gap: nothing
in the shipped app ever retained the copyright/license notices its MIT,
Apache-2.0, BSD, ISC, etc. dependencies generally require distributing
alongside a compiled binary. Not a GPL-class blocker — none of this app's
~625 transitive Rust crates or 12 bundled JS packages carry a copyleft
license (verified below) — but a routine compliance gap worth closing
rather than leaving informally "probably fine."

## Decision

**Generate a real notices page from the actual dependency graph, not a
hand-maintained list.** A hand-written list goes stale the moment anyone
runs `cargo add`/`npm install` — exactly the kind of drift that let
ADR 0033's problem happen in the first place. Two generators, one for each
ecosystem, combined by `scripts/generate-third-party-notices.sh` into
`apps/desktop/src-tauri/resources/third-party-notices.html`, bundled via
`tauri.conf.json`'s `bundle.resources` and opened from a new Help menu
item ("Open Source Licenses") via `tauri_plugin_opener::open_path` in the
user's default browser — a 13,000+ line reference document isn't something
worth reimplementing as an in-app modal the way the much shorter Keyboard
Shortcuts list is.

- **Rust: `cargo-about`** (installed as a dev tool via `cargo install
  cargo-about --locked --features cli`, not a runtime dependency of any
  crate — it never touches the shipped binary or its dependency tree).
  Configured via `about.toml`: every atomic SPDX identifier actually
  present in `cargo metadata`'s full dependency graph (checked directly,
  not guessed) is in `accepted`; `[private] ignore = true` excludes this
  workspace's own Proprietary crates from the report. That required adding
  `publish = false` to every workspace crate — not just a workaround for
  the tool, a correct thing to have regardless, since none of these crates
  is ever meant to reach crates.io.
- **JavaScript: a ~150-line hand-written script**
  (`scripts/gen_js_notices.py`), not a new tool. Considered `license-checker`
  and similar, but the actual scope here is 12-13 resolved production
  packages (`npm ls --all --omit=dev`, walking the full resolved tree, not
  just direct dependencies) — small enough that this repo's own established
  preference (small hand-written logic over a new dependency when the
  problem is this bounded — see ADR 0025 and friends) applies here too.
  Reads each package's own bundled `LICENSE`/`LICENSE.md`/etc. file
  directly; three `@tauri-apps/plugin-*` packages only ship a `LICENSE.spdx`
  metadata stub, so those fall back to the identical MIT text bundled by
  their co-installed sibling `@tauri-apps/api` — confirmed byte-identical
  against the actual upstream `tauri-apps/plugins-workspace` repository
  file before relying on it, not assumed from "probably the same org."
  Refuses to generate (non-zero exit) rather than silently omitting
  anything if a resolved package has no license text available anywhere —
  a missing notice should be a loud failure, not a quiet gap.

**Verified: no copyleft anywhere, in either ecosystem.** `cargo metadata`'s
full 625-package graph and the resolved npm production tree were both
checked directly for GPL/AGPL/LGPL-family identifiers — the only near-hit,
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` on 2 crates, offers LGPL only as
one alternative among others already in `accepted`, not a forced
obligation. This is the same full-graph sweep the ADR 0033 investigation
started as a targeted question ("is there another function with the same
issue") and is now a repeatable, scriptable check instead of a one-time
manual grep.

## Consequences

- `scripts/generate-third-party-notices.sh` needs to be re-run by hand
  whenever a dependency changes, and definitely before cutting a release —
  nothing regenerates it automatically today (no pre-commit hook, no CI
  step). A stale copy doesn't error, it just quietly under-reports what's
  actually shipping, so this is a real, if low-severity, maintenance
  burden to remember. A future pass could wire it into CI as a "does this
  PR touch Cargo.lock/package-lock.json without regenerating notices"
  check; not done here.
- `cargo-about` is a one-time local install (`~/.cargo/bin`), not tracked
  in this repo and not something CI needs unless the regeneration check
  above gets built.
- The generated file is committed (`apps/desktop/src-tauri/resources/third-party-notices.html`),
  not built fresh on every `tauri build` — consistent with how `models/*`
  and the document icons are already committed, versioned resources rather
  than generated at build time.
