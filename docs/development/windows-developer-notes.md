# Windows developer — notes from macOS

Messages from the macOS dev to the Windows dev. Short entries only.
Reply in [macos-developer-notes.md](macos-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

---

## 2026-09-13 — fifth fix, same pull as below — still haven't seen anything from you

One more, small: `process_one_waveform_job` (`apps/desktop/src-tauri/src/lib.rs`) now checks
`get_waveform_cache` before doing any decode/build work — if an asset already has a valid cache
it completes the job directly instead of redoing it. Belt-and-suspenders on top of fix #3 below:
catches an abandoned claim from before a restart whose prior run actually finished writing the
cache but never marked the job completed, plus any stale duplicate job rows already sitting in a
library from before fix #3 landed. Commit `aad3431` — pull `main` now to get all five fixes.

## 2026-09-13 — third and fourth fixes on top of the two below — still haven't seen a pull from you, please grab everything at once

Two more critical fixes since the waveform-CPU one below. If you haven't pulled since the
tracks-vanishing fix (`2fb8550`), pull `main` now (`89b1b6b`) to get all four in one rebuild:

1. **Waveform jobs never actually completed** (`crates/storage/src/lib.rs`,
   `complete_pending_jobs_for_asset`): the SQL only matched `state = 'pending'`, but the
   standalone waveform-job path calls it after the job's already `'processing'` — so it never
   matched, the job sat `'processing'` until the 3-minute stuck-job timeout put it back to
   `'pending'`, and it got reclaimed and rebuilt forever. This is the "waveform generation never
   stops, keeps reprocessing already-done tracks" bug — commit `7d8d2be`.
2. **Multi-selecting several tracks, then adding them to a Project (drag or the sidebar button)
   only added a fraction of them, or none** (`apps/desktop/src-ui/src/App.tsx` +
   `crates/workspace-state/src/lib.rs`): the real multi-selection lives in `browserState`, kept
   separate from `selectedAssetId` (just "last clicked row"). The effect that rebuilds
   `browserState` on every asset-list refresh only ever restored the single `selectedAssetId`,
   silently collapsing a real multi-selection down to one row (or zero) whenever a refresh landed
   between selecting tracks and clicking "add to project" — which on a library with any real
   background job backlog is often. Added `BrowserCommand::SelectIndices` to restore the full
   previous selection in one call — commit `89b1b6b`.

Both are pure Rust/TS logic, not platform-specific — please ship whatever you build after this
pull as 0.3.1 build 13 or higher (check the ledger first, as always).

## 2026-09-13 — second fix on top of the one below: waveform generation was stealing CPU and disrupting selection

Same day, one more before you build — pull `main` again (commit `7aaad82`, on top of the
tracks-vanishing fix at `2fb8550` below) to get this too, so your rebuild carries both in one pass
instead of needing a second round-trip.

Two bugs compounded into "waveform generation never finishes and messes with the UI while it
runs": `WaveformCache::from_samples` (`crates/waveform/src/lib.rs`) built a one-bucket-per-raw-
sample intermediate array — tens of millions of entries for an ordinary track — just to downsample
it three times and discard it. Fixed to generate each of the three needed resolutions directly from
the sample buffer instead; this is the actual reason it was slow regardless of CPU/network. Separately,
the frontend's `runJobDrain` (`apps/desktop/src-ui/src/App.tsx`) refreshed the whole asset list after
every waveform batch even though waveform data isn't part of `AssetRecord` at all — since this job
kind drains almost continuously on any real backlog, that churn kept re-triggering the "selection no
longer in the list" effect, deselecting/reordering tracks out from under whoever was browsing.
Waveform batches no longer trigger that refresh, and `refreshAssets` now guards against out-of-order
responses generally.

Not platform-specific — please ship whatever build follows this pull as 0.3.1 build 13 or higher
(same version as the tracks-vanishing fix below, since neither has been Windows-built yet), and
check `docs/macos/version-ledger.md` first per the usual rule.

## 2026-09-13 — critical fix: tracks vanishing from the canvas after a library swap — please pull and rebuild

Pull `main` (commit `2fb8550` on top of your `RunEvent::Opened` fix, `4b59cd1`) before your next
Windows build. Root cause: `process_one_audio_analysis_job` / `process_one_waveform_job` /
`process_one_instrument_job` release the catalog mutex across their decode/DSP/model-inference
`.await` (correct — holding it there blocks the whole app), then re-lock it afterward to persist
the result. If the active library was swapped mid-job — via the new File > Open Library / Last
Open menu items, or the autonomous overnight-drive loop — that re-lock silently grabbed a
*different* `Catalog` than the one the job was claimed from. Storage's `UPDATE ... WHERE id = ?`
matched zero rows there (never checked), so the result was dropped while the job still reported
"succeeded", and the frontend then refreshed the canvas with a stale library ID against the new
catalog — wiping already-imported tracks, not just the ones mid-analysis. Not platform-specific
(pure Rust logic bug), so it affects Windows the same way; please rebuild and ship 0.3.1 build 13
(or higher, following the ledger) once you've pulled this.

## 2026-09-13 — `deploy_direct_windows.ps1` now checks the version ledger before building

Marketing version bumped `0.3.0` → `0.3.1` (all three: `tauri.conf.json`/`package.json`/`Cargo.toml`)
— App Store Connect rejected a 0.3.0 resubmission (already approved). Not Windows-specific, just
context for the version jump.

The actual thing to know: `deploy_direct_windows.ps1` now dot-sources a new
`scripts/check_version_ledger.ps1` as its first step, which reads `docs/macos/version-ledger.md`
(same file the macOS scripts already check) and **throws before building anything** if the local
`tauri.conf.json` version doesn't exceed the last row marked `Approved` there. This exists because
the exact same "reused an already-approved version" mistake happened twice on the Mac side. Despite
living under `docs/macos/`, that ledger is the one shared source of truth for the version every
platform builds from — nothing Windows-specific needed here, just pull before your next build so
you're not blocked by a stale local `tauri.conf.json` version the ledger already knows is unsafe.

Please sanity-check on Windows after pulling: run `deploy_direct_windows.ps1` once and confirm
`[0/4] Checking version against docs/macos/version-ledger.md...` prints and passes (or fails with
a clear message, if you haven't pulled the version bump yet) before `[1/4]` starts — this is plain
PowerShell (no new deps), but I can't execute `.ps1` from macOS to verify it directly.

---

## 2026-09-12 (later) — new: license PDF attach/expiry tracking (new crate, needs your eyes); one correction below

Added a new workspace crate, `crates/license-documents` (pure Rust, no system libraries — `pdf-extract`/`lopdf` for PDF text extraction, `regex` for date-keyword scanning, `chrono` for date math). Lets a user attach a track's bundled license PDF; the app copies it into `<library media_root>/License/`, best-effort extracts candidate expiry dates as suggestions (never auto-applied — the user always confirms), and shows a valid/expiring-soon/expired badge in the Source & License inspector panel plus a library-wide Maintenance finding.

**First `cargo build`/`cargo check` after pulling will fetch ~15 new crates** (aes/cbc/ecb/sha2/md-5/ttf-parser/etc. — all pulled in transitively by `lopdf` for encrypted-PDF support, all pure Rust, no `vcpkg`/system-library dependency) — expect a longer first build, not a failure. I can't verify the actual Windows compile from macOS, though every dependency here is a widely-used, routinely-cross-platform-built crate (no red flags in `cargo tree`).

Please sanity-check on Windows after pulling:

- `cargo build`/`cargo test -p license-documents` actually succeeds (this is the part I can't verify from here).
- In the app: select a track, Source & License panel → "Attach License PDF…" — confirm the native file dialog filters to `.pdf`, the file lands in `<media_root>\License\`, "Reveal in Explorer" opens the right folder, and the valid/expired badge renders correctly.

**Correction to the 2026-09-12 (earlier) entry below**: the macOS side of the keep-awake fix changed since that note — it no longer spawns `/usr/bin/caffeinate` (that silently fails under the Mac App Store's sandbox; App Sandbox blocks spawning any binary not embedded in the app bundle). It now calls `IOPMAssertionCreateWithName` directly (verified against real `pmset -g assertions` output, sandboxed and non-sandboxed both). **This doesn't touch the Windows `SetThreadExecutionState` path at all** — that code is unchanged, and the sanity-check ask below is still open (no reply in `macos-developer-notes.md` yet).

Also: marketing version bumped `0.2.1` → `0.3.0` (`tauri.conf.json`/`package.json`/`Cargo.toml`, all three move together) — App Store Connect rejected a same-version resubmission (0.2.1 was already approved). Not a Windows-specific concern, just context for why the version number moved. See `docs/macos/version-ledger.md` if curious — it's the new source of truth for what's been built/uploaded/approved on that channel.

`cargo check`/`cargo test`/`cargo clippy` all pass on macOS for both the default and `direct-dist` feature builds (`cargo build --release` too, both feature sets).

---

## 2026-09-12 — new: keep-awake during overnight analysis batches (needs your eyes on Windows)

Added `apps/desktop/src-tauri/src/power.rs` + a new autonomous job-drive loop in `lib.rs`'s
`setup()`. Goal: a big analysis batch queued up and left to run overnight shouldn't get cut short
by the OS idle-sleeping the machine. Two platform paths, no new deps:

- macOS: spawns `/usr/bin/caffeinate -i` while there's unpaused pending work, kills it when the
  queue empties/pauses/the window closes. Verified locally (unit tests in `power.rs` spawn a real
  `caffeinate` and confirm it lives/dies correctly) and by watching `ps aux | grep caffeinate`
  during a real analysis batch.
- Windows: calls `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` to engage, back to
  `ES_CONTINUOUS` alone to release — via a hand-rolled `extern "system"` binding to `kernel32`
  (see `power.rs`'s `#[cfg(target_os = "windows")] mod imp`). **I can't verify this from macOS.**

Please sanity-check on Windows after pulling: queue up a handful of pending analysis jobs (import
something, or use a library with a backlog), let it idle a bit while jobs are processing, and
confirm via Task Manager → Details (or `powercfg /requests` in an elevated prompt) that
`darkwave-desktop.exe` is holding an `ES_SYSTEM_REQUIRED` request while jobs are pending, and that
it goes away once the queue drains or you pause analysis in the UI. There's also a new "Keep the
computer awake while analyzing" toggle in Settings → General if you want to confirm the opt-out
path too (should mean no request is ever held even with jobs pending).

`cargo check`/`cargo test`/`cargo clippy` all pass on macOS for both the default and
`direct-dist` feature builds; nothing new needed on the Rust side beyond a pull.

---

## 2026-09-11 — main synced again, pull now (bigger batch)

`main` updated: `918e5b5` → `838d32d` — a long session, several real
backend bugs + hardening. Pull it:

```
git pull origin main
```

No new deps, no new models. Code + one config change only.

**One thing that actually needs your eyes: the CSP.** `tauri.conf.json`'s
`security.csp` went from `null` to a real policy. I derived it from
Tauri's own source for both platforms (macOS uses bare `asset://` /
`ipc://` schemes, Windows resolves to `http://asset.localhost` /
`http://ipc.localhost` since `useHttpsScheme` isn't set) and included
both forms — but I could only live-verify playback on macOS today
(selected a track, confirmed the waveform rendered and the clock
advanced). Please do the same basic check on Windows after pulling:
select a track, hit play, confirm it actually plays and the waveform
renders. If it's silently broken there, it's almost certainly this CSP
— log it in macos-developer-notes.md and I'll adjust.

What else changed (context, not action items):

- Several real job-queue bugs fixed (silent stuck jobs on pause,
  premature job-reset, a stuck-forever fingerprinting step decoupled
  from job completion, VAD reloading its model per file)
- `[profile.release]` added (LTO + single codegen unit + stripped) —
  release builds should be a genuinely faster binary now; also means
  release builds take longer to compile than before (verified: ~2min
  for darkwave-desktop alone on this Mac)
- Orphaned preview-cache files now cleaned up on trash deletion
- Dropped an unused npm dependency, fixed an npm audit advisory

Action for you: pull, rebuild (release build will be slower now — see
above, that's expected), confirm playback works. Flag anything
Windows-specific below.

---

## 2026-09-11 — main synced, pull now

`main` updated: `db9147b` → `918e5b5`.

Pull it:

```
git pull origin main
```

No new deps. No new Rust crates. No model files needed. Code only.

What changed (for context, not action items):
- Tag suggestions, project drag-and-drop, project edit icon
- Sonic Radar Analyze buttons fixed (Pitch/Instruments were silently failing)
- Waveform-generation pause toggle + lower concurrency
- 3 backend bugs fixed that stuck instrument-detection jobs in "processing" forever

Action for you: just pull, rebuild, confirm it launches and a track selects
without hanging. No Windows-specific work expected here — flag it below if
something breaks only on Windows.
