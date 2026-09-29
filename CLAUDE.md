# Darkwave — CLAUDE.md

This file currently only documents the release process — nothing else about the project has been
written up here yet. Extend it as other conventions get established.

## Every release updates the self-served store too

The self-served store (`alan-design.com/#/store`) is managed by a separate project, **Store Manager** (`/Users/alan/_localDEV/_creative/_store_manager`), not by anything in this repo. It's a real, running system (TrueNAS, always-on) with its own watch → review → publish pipeline — see its own `CLAUDE.md` and `docs/runbook.md` before touching it.

**Every version bump runs both build tracks, not just one.** It's tempting to treat "building for App Store review" and "shipping direct-sale" as separate occasions — they aren't. Whichever reason triggered the build (an MAS submission, a direct-sale release, a Windows-only drop), **do the full cycle every time**: MAS build (`mac_sign_and_package_mas.sh`) *and* direct build (`deploy_direct_macos.sh`) *and* the Store Manager ingest drop below, for that same version. Concretely this means:

- Building for MAS review does **not** skip the Store Manager step — drop the direct macOS build there too, same version, so the storefront's pending version tracks whatever's currently in Apple's queue instead of lagging behind it.
- A drop that's missing one platform (e.g. a Windows-only ingest, or a macOS-only one) isn't done — add the other platform's file to the *same* version's manifest as soon as it's built, rather than leaving that version's Inbox entry partial. This actually happened on 2026-09-12: a Windows-only `0.3.0` manifest sat in Store Manager's Inbox for a day with no `macos` entry because a Mac-side build hadn't run yet — don't let that gap recur.
- Skipping this because "it's just an MAS build" is exactly how the storefront ends up selling a stale version under an unrelated listing. Nothing syncs this automatically.

1. Run `bash apps/desktop/scripts/deploy_direct_macos.sh` as usual — it builds, signs, notarizes, and staples, same as before. It does **not** copy the DMG into `licensing-server/releases/` anymore; the script's own final output tells you the Store Manager ingest path to use instead.
2. Drop a `manifest.json` (see Store Manager's `docs/manifest-schema.md`) + the DMG into `/Volumes/Gaia/04_DEV/store-manager/ingest/Apps/darkwave/<version>/` — filename must be the fixed `Darkwave.dmg`, not version-suffixed, since `products.js`'s existing `darkwave` entry already points at that literal path and a republish of an already-registered slug leaves that file untouched (no-op), so the manifest's declared filename has to already match it.
3. Within ~30s it appears in Store Manager's dashboard Inbox (`http://192.168.178.146:5180`) as `pending_review`. **A human reviews it — Dry-run first, then Publish. Don't script around this step**; it's a deliberate gate since Publish touches live Stripe records and rewrites `web_three`'s registry files.
4. Confirm the dry-run says "reusing existing price" (Darkwave's existing Stripe product/price, `prod_V05OcMBlCmhAjL` / `price_1U05EpCsCSs3k4X1fr7x7aER`, is already seeded into Store Manager's database) — not creating a new one.
5. Follow the deploy checklist Store Manager prints after Publish. See Store Manager's `docs/runbook.md` § "TrueNAS (production)" for known gotchas (git LFS not on the TrueNAS host PATH, the staging clone's `safe.directory` requirement, the real deploy path being `store-manager/app/` not `store-manager/`, and backend deploys to `web_three` being rsync-from-the-Mac via `deploy.sh`, never git-on-TrueNAS).

Windows builds ship unsigned for now (deliberate, already-made call — see `docs/development/release-readiness.md` if that decision is ever revisited). The same Store Manager manifest can carry a `windows` file entry alongside `macos` in one drop if both platforms are ready together — see `docs/manifest-schema.md`.

## Apple release accounts & IDs

Every past "notarization 401" / "Transporter rejected" incident came from using the wrong Apple ID or a stale password. The facts, once:

- **Developer Apple ID: `alan.creative@icloud.com`** — the account for App Store Connect, notarization, and Transporter. **Not** the machine's personal login (`alanxalves@me.com`). The app-specific password for notarization must be generated at appleid.apple.com *while signed in as `alan.creative@icloud.com`*.
- **Team ID:** `RD7UU4Z3D2` (Nudson Alan Terrinha Alves). **App Store Connect app ID:** `6797313803`.
- **Bundle IDs:** `dev.darkwave.app` (MAS) · `dev.darkwave.app.direct` (direct-sale).
- **Notary keychain profile:** `darkwave-notary`. Apple revokes app-specific passwords periodically → `deploy_direct_macos.sh` fails at `[3/5]` with HTTP 401. Fix: regenerate the password, then `xcrun notarytool store-credentials darkwave-notary --apple-id alan.creative@icloud.com --team-id RD7UU4Z3D2` (omit `--password`, let it prompt), and confirm with `xcrun notarytool history --keychain-profile darkwave-notary` before retrying.
- **Signing certs** (login keychain): Developer ID Application `2FDD1878…` (direct, pinned by SHA-1 in the script) · `3rd Party Mac Developer Application` + `… Installer` (MAS).
- **Every App Store upload needs a fresh build number** — bump `CFBundleVersion` in `apps/desktop/src-tauri/Info.plist` (marketing version comes from `tauri.conf.json`, also mirrored in `package.json` and `Cargo.toml`). App Store Connect 409s a re-used `(CFBundleShortVersionString, CFBundleVersion)` pair, and `CFBundleShortVersionString` must exceed the last *approved* version — **check `docs/macos/version-ledger.md` before bumping either one**, it's the actual record of what's been built/uploaded/approved (this exact 409 happened twice — 0.2.1/10 on 2026-09-12, 0.3.0/11 on 2026-09-13 — both times because the ledger's Status field wasn't updated once approval actually landed). `scripts/mac_sign_and_package_mas.sh` appends a row automatically every time it signs a `.pkg`; update that row's Status by hand once Transporter reports the outcome. All three build scripts (`mac_sign_and_package_mas.sh`, `deploy_direct_macos.sh`, `deploy_direct_windows.ps1`) now check the ledger themselves before building (`check_version_ledger.sh` / `.ps1`) and refuse to proceed if the version/build would repeat something the ledger already shows — this catches the mistake before a wasted build, but it's still only as good as the Status field a human keeps current.
- **App Store listing name:** `Darkwave — Sound Library` (plain "Darkwave" is taken — App Store names are globally unique).

## Background jobs — never requeue an unreachable file straight to `pending`

This happened on 2026-09-13: the standing background worker (`apps/desktop/src-tauri/src/lib.rs`,
spawned in `.setup()`) drives `process_audio_analysis_jobs`/`process_waveform_jobs`/
`process_instrument_jobs` on a ~1 second cycle for as long as the app is open. A job whose asset
file wasn't reachable yet (unmounted NAS share, external drive, a Referenced path not warmed into
the local cache) used to be requeued straight back to `'pending'` with zero backoff — the very
next 1-second tick reclaimed it and hit the same unreachable file again, forever, invisibly (no
error, no stop). On a 3526-file library this looked exactly like "background analysis loops
forever, never stops, occasionally fails but keeps going" — fixed in PR #4/#5.

**The rule going forward:** any job kind that can hit a file that simply isn't reachable *yet*
(as opposed to a real processing failure — corrupt file, decode error, etc.) must call
`defer_unavailable_job` (`apps/desktop/src-tauri/src/lib.rs`), never
`catalog.requeue_job_as_pending`. `defer_unavailable_job` leaves the row `'processing'` and counts
a silent retry via `mark_job_attempt`, paced by `reset_stuck_processing_jobs`'s existing 3-minute
age floor — so a retry can happen at most every ~3 minutes, not every second — and only fails the
job for real (visibly, recoverable via "Retry Failed Jobs") after
`MAX_SILENT_AVAILABILITY_ATTEMPTS` (20, ~1 hour). `requeue_job_as_pending` is reserved for a
caller that knows the claim was abandoned for a specific, immediate reason and wants it reclaimed
right away (e.g. the queue being paused mid-batch) — using it for "not ready yet, try later" is
exactly what causes this bug.

The silent-retry counter lives in its own `background_jobs.availability_attempts` column,
deliberately separate from `attempts` (which `fail_job` increments and which the real-failure
auto-retry cap in `requeue_failed_jobs` checks against) — sharing one counter between "waiting for
a file to reappear" and "an actual attempt that failed" lets a merely-slow NAS mount burn through
a file's real-failure retry budget before real processing ever runs once.

## Canvas renders 0 rows despite tracks existing — check the viewport measurement, not the data

This happened on 2026-09-17: after importing a batch of sounds, the browser (the main track list —
"canvas" in commit messages/conversation) showed nothing at all, even though the import had fully
succeeded. The asset rows were confirmed present and correctly saved by reading the `.darkwave`
file directly with `sqlite3 <path>.darkwave "SELECT count(*) FROM assets WHERE library_id='<id>';"`
— **do this first** whenever "tracks aren't showing up" is reported, to immediately split the
problem into "backend/import actually failed" vs. "the frontend has the data but won't render it."
The status bar under the browser (`apps/desktop/src-ui/src/App.tsx`, the `"{rows} rows"` /
`"{rendered} rendered"` line) makes this same split visible at a glance: a real empty *filter*
result shows `0 rows`; this bug class shows a nonzero row count with `0 rendered`.

**Root cause:** the row-virtualization viewport height (`browserViewportHeight`) was tracked by a
`ResizeObserver` set up inside a `useEffect(() => {...}, [])` — a one-time effect that captures
whichever DOM node `browserScrollRef.current` happens to be at first mount and never re-runs. The
browser `<section>` fully unmounts whenever the sidebar's Instrument Detection page is showing
(`instrumentFilter?.instrumentPage`) and remounts as a **new** DOM node when the user navigates
back to any other view. The one-time effect never reattaches to that new node, so the observer is
left permanently watching a stale, detached element (WebKit fires one final resize event reporting
0×0 when an observed node leaves the document) — `browserViewportHeight` gets stuck, and
`computeVisibleRowRange` correctly computes zero visible rows for a viewport it now believes is
0px tall, forever, for the rest of that session. This has nothing to do with import, search,
filters, or the library-file model — any navigation that unmounts and remounts the browser section
would trigger it.

**The rule going forward:** never pair `useRef` + `useEffect(..., [])` to attach a `ResizeObserver`
/ `IntersectionObserver` / any DOM measurement to a node that isn't guaranteed to share the owning
component's mount lifecycle. Use a **callback ref** instead (see `attachBrowserScrollNode` in
`App.tsx`) — it fires with the actual current node on every mount *and* unmount, so a disconnect
followed by a fresh `observe()` happens automatically no matter what unmounted/remounted it or how
many times.

Two hardening layers now guard this specific spot against a recurrence from a *different* cause:

1. The observer callback ignores a reported height of exactly `0` — a mounted, on-screen browser
   section never legitimately has zero height in this layout (it's a flex-grow panel, never
   `display:none`'d while mounted), so `0` is always a detach/measurement artifact, never a real
   "the list is now 0px tall."
2. A canary effect (`console.warn`, tagged `[browser]`, right after `browserVisibleRange` is
   computed) fires whenever rows exist but the computed render range is empty — pointing straight
   back at this section instead of leaving a silent blank canvas with no lead at all.

## Exporting to a project folder — never fan out across a track's projects

A track can be in any number of projects (`collection_assets` has no per-asset uniqueness, on
purpose — one cue really does get used in a trailer, a director's cut, and a social edit). So
"send this to its project folder" has no single answer, and until 2026-09-23 the per-row send
button answered it by sending to *all* of them: outside a project view it did
`Promise.all(memberships.filter(hasAnyFolder).map(export…))`, copying one file into every
project's export folder in a single click. Those are live watch folders for Resolve/Premiere —
an unexpected file there may be on someone's timeline before anyone notices it shouldn't be.

**The rule going forward:** every send names exactly one project. `sendAssetsToProject`
(`App.tsx`) is the only frontend caller of `export_asset_to_project` and takes a single project,
never a list — keep it that way, so fan-out stays structurally impossible rather than merely
avoided. `resolveSendTarget` picks the target by precedence (browsed project → active project →
the only candidate with a folder) and returns `choose` when none of those settles it; `choose`
opens the picker. Fanning out survives only as an explicitly labelled "Send to all N projects"
item inside that picker. See `docs/adr/0035-single-project-export-targeting.md`.

Two things that follow from this and are easy to get wrong again:

- **Per-project export state is per project.** `usage_events.project_id` has always recorded
  exactly which project a file went to, and `project_memberships_for_library` returns `exported`
  per membership — but the row badge used to collapse it with `memberships.some(m => m.exported)`,
  so sending to one project marked the track green in all of them. Any UI showing "already sent"
  must name the project it means.
- **The active export target lives in preferences, not the library file.**
  `AppPreferences::active_project_by_library` is per-user working state ("what am I cutting
  today"), not library data that should travel to another machine with the `.darkwave` file.
  Resolve it against the live `collections` list rather than trusting the stored id, and call
  `forget_active_project` if a project-deletion command is ever added.

## Keep every dev machine on the same version — branch hygiene

Development happens on more than one machine (this Mac, and a Windows machine — see
`docs/src/content/docs/development/windows-setup.md`, which already assumes `git pull origin main`
is how Windows picks up changes). That only works if `main` on GitHub is actually kept current and
every machine's local branches are pushed, not left sitting locally.

- **`main` is the single source of truth.** Every machine pulls from it; nothing should be "ahead"
  of GitHub for more than a session. If a local branch exists (feature work, a Windows-specific
  build fix, anything), **push it the same session it's created** — an unpushed branch on one
  machine is invisible to every other machine and to Claude sessions running elsewhere, which is
  exactly the kind of drift that causes "why doesn't this build on Windows" surprises later.
- **Machine-specific fixes still go through a real branch + PR, not a local-only branch.** If the
  Windows machine needs changes just to build there (a `Cargo.lock` platform quirk, a path-handling
  fix, a toolchain workaround), make them on a short-lived branch off `main`, push it, open a PR,
  and merge it back into `main` promptly — don't let it live only as an unpushed local branch. A
  fix that isn't merged doesn't exist as far as any other machine is concerned.
- **Merge and delete branches once their work lands — don't let them linger.** A long-lived branch
  that's diverged from `main` for days/weeks (commits piling up on one side with no PR) is the
  streamlining problem, not a feature. If a branch is meant to ship, open the PR as soon as there's
  something reviewable, not after it's accumulated a pile of unrelated follow-on work.
- **Before doing git work in a session** (committing, branching, or being asked to "streamline" /
  "verify the repo"), run `git fetch --all --prune` and check `git branch -vv` / `gh api
  repos/{owner}/{repo}/branches` against what's expected — a branch a human mentions (e.g. "the
  Windows branch") but that doesn't show up on GitHub means it's still local-only somewhere and
  needs pushing before it can be reviewed, merged, or even inspected from another machine.

## Cross-machine dev notes

Two files carry short messages between the macOS and Windows devs — no live channel needed:

- `docs/development/windows-developer-notes.md` — macOS writes, Windows reads/acts.
- `docs/development/macos-developer-notes.md` — Windows writes, macOS reads.

Newest entry on top. Don't edit old entries — append. Keep entries short: what changed, what to
do, nothing else. After a session that lands a sync-worthy change (new deps, new required files,
a Windows-only ask), add an entry to the Windows doc **and commit + push it with the rest of the
work**, same as any other branch-hygiene change above — an unpushed note is as useless as an
unpushed branch.
