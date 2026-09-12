# App Store version ledger

Single source of truth for "what's the next safe `(version, build)` pair" — kept because
[`NEXT_STEPS.md`](NEXT_STEPS.md)'s own checklist drifted out of sync with reality (it still
said "Build 2 uploaded" while the repo was actually on build 10), which is exactly how this
ledger's first entry below happened: nothing recorded that version 0.2.1 had already been
approved, so a same-version resubmission got rejected.

**The rule**: `CFBundleShortVersionString` (marketing version, `apps/desktop/src-tauri/tauri.conf.json`
→ `version`, also mirrored in `package.json` and `Cargo.toml`) must exceed the last **approved**
version below — not just the last *uploaded* one. `CFBundleVersion` (build number,
`apps/desktop/src-tauri/Info.plist`) must simply be unused, full stop, regardless of whether the
marketing version changed. Before bumping either, check the most recent row here.

**Keeping this current**: `scripts/mac_sign_and_package_mas.sh` appends a `Built` row
automatically every time it produces a signed `.pkg` — that part won't go stale. What it can't
know is what happens after you hand the file to Transporter, so **update that row's Status by
hand** once you know the outcome (`Uploaded` → `Approved` / `Rejected (reason)`). Newest entry on
top; don't edit old rows, append.

| Date | Version | Build | Status | Notes |
|------|---------|-------|--------|-------|
| 2026-09-12 | 0.3.0 | 10 | Uploaded | License PDF attach/expiry tracking + sandbox-safe sleep-prevention fix (caffeinate → IOPMAssertionCreateWithName; the old approach silently failed under the App Sandbox). Transporter log confirmed `CREATE BUILD` 201 (created) and the asset description upload reaching `COMPLETE` state — delivery to App Store Connect succeeded. Still waiting on Apple's review decision (Approved/Rejected not known yet). |
| 2026-09-12 | 0.2.1 | 10 | Rejected (409) | Transporter: "must contain a higher version than that of the previously approved version [0.2.1]" — confirms 0.2.1 itself was already approved at some earlier build (exact approved build number not recorded pre-ledger; 8, 9, or 10's predecessor). Superseded by the 0.3.0/10 row above — needed a version bump, not just a build bump. |
| 2026-09-01 | 0.2.1 | 9 | Uploaded (outcome not recorded) | Bumped from build 8 per commit `d0d3869` ("build 8 already registered in App Store Connect"). |
| 2026-09-01 | 0.2.1 | 8 | Uploaded (outcome not recorded) | First 0.2.1 build, per commit `47a4ba9` ("App Store 0.2.0 was already approved"). Also the commit that fixed the icon padding (`fc15ec3`) and the missing `network.client` entitlement blank-screen rejection (`caaf3f8`) landed in this 0.2.1 cycle. |
| — | 0.2.0 | — | Approved | Inferred from commit `47a4ba9`'s message; no earlier ledger entry exists for it. |

## Where each version lives (must all move together)

- `apps/desktop/src-tauri/tauri.conf.json` → `version` (this becomes `CFBundleShortVersionString`)
- `apps/desktop/package.json` → `version`
- `apps/desktop/src-tauri/Cargo.toml` → `[package] version`
- `apps/desktop/src-tauri/Info.plist` → `CFBundleVersion` (build number only, not the marketing version)

## direct-dist / Store Manager

This ledger is App Store Connect–specific — the direct-sale channel has no equivalent
version-monotonicity gate (see `CLAUDE.md` § "Every release updates the self-served store too"),
so it isn't tracked here.
