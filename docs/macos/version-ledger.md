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
top; don't edit old rows other than that Status field — this has now bitten twice (0.2.1/10 on
2026-09-12, 0.3.0/11 on 2026-09-13): a row sits at `Uploaded` because nobody checked back, the next
build reuses that same marketing version since nothing said otherwise, and Apple's 409 is the
first sign the version was actually approved days or even hours earlier. **Before starting a new
build, don't just read the latest row's Status — if it says `Uploaded` and any real time has
passed, check App Store Connect directly** (or ask whoever's been watching Transporter/email) for
whether it's since been approved, rather than trusting a Status field that only updates when
someone remembers to.

**Enforced automatically, not just documented**: `scripts/check_version_ledger.sh` parses this
file's table directly — the last row whose Status starts with `Approved`, and the highest build
number ever recorded regardless of status — and refuses to build (exits non-zero before any
compiling starts) if the current version doesn't exceed that approved version, or the current
build number isn't higher than every build number already used. `mac_sign_and_package_mas.sh`
and `deploy_direct_macos.sh` both source it as their first step. `check_version_ledger.ps1` is
the same check for `deploy_direct_windows.ps1` (version only — there's no Windows equivalent of
App Store Connect's build-number constraint), reading this exact file over the shared git
history so Windows can't drift onto a version macOS's ledger already shows is unsafe. This
still can't know an approval happened before a human flips a row's Status by hand (see above) —
it only stops the *specific* mistake of reusing a version/build the ledger already knows about.

| Date | Version | Build | Status | Notes |
|------|---------|-------|--------|-------|
| 2026-09-13 | 0.3.1 | 12 | Built | Not yet uploaded — update this row once Transporter tells you the outcome. |
| 2026-09-13 | 0.3.0 | 11 | Rejected (409) | Transporter: "must contain a higher version than that of the previously approved version [0.3.0]" — confirms 0.3.0 itself was approved (at build 10, below) sometime between 2026-09-12 and 2026-09-13, faster than expected. Same root cause as the 0.2.1/10 rejection below: this row's Status was never flipped from "Uploaded" to "Approved" once it actually was, so the next build reused the now-approved marketing version instead of bumping it. Superseded by the 0.3.1/12 row above — needed a version bump, not just a build bump. |
| 2026-09-12 | 0.3.0 | 10 | Approved | License PDF attach/expiry tracking + sandbox-safe sleep-prevention fix (caffeinate → IOPMAssertionCreateWithName; the old approach silently failed under the App Sandbox). Transporter log confirmed `CREATE BUILD` 201 (created) and the asset description upload reaching `COMPLETE` state — delivery to App Store Connect succeeded. Approval inferred from the 0.3.0/11 rejection above (Apple's 409 names 0.3.0 as "the previously approved version") — not from a direct App Store Connect check, so confirm there if in doubt. |
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
