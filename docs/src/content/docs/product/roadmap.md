---
title: V1 Readiness Review
description: Full spec-vs-implementation audit ahead of the first Windows trial and a V1 decision.
---

_Reviewed 2026-07-31, against `docs/genesis/audio_library_application_plan.md` (updated the same day), ADRs 0001–0027, and direct inspection of the current codebase. This is a review, not new development — nothing described as "Missing" below was built as part of this pass, except two small hardening fixes called out explicitly in Section 0._

Legend: **Complete** · **Partial** (real, working, but not the full scope requested) · **Missing** (not built).

---

## 0. GitHub-sourced feature audit (requested specifically, ahead of V1)

Two real features landed since the plan was last annotated (`80531ab` → `HEAD`): the DaVinci Resolve quick-export button and Silero VAD vocal detection. Both were audited end-to-end.

### DaVinci Resolve quick-export — **Complete, no defects found**

- Frontend: `handleExportToProject` (`apps/desktop/src-ui/src/App.tsx:1004-1020`) guards empty selection and missing `export_path`, batches via `Promise.all`, and surfaces the real error message on failure rather than swallowing it.
- Backend: `export_asset_to_project` reuses the existing, already-tested `export_pipeline::plan_editorial_export`/`execute_original_copy_export` seam rather than duplicating logic.
- No race conditions, no silent failure paths. This is a thin, well-built UI on top of an already-Done capability (Milestone 6's "copy to project media folder").

### Real vocal detection (Silero VAD) — **Working, one real bug found and fixed, one architectural risk flagged**

- New ADR written: `docs/adr/0027-real-vocal-detection-silero-vad.md`.
- **Bug found and fixed this pass:** the per-selection `asset_vocal_ratio` fetch (`App.tsx`, in the `useEffect` keyed on `selectedAssetId`) had no protection against out-of-order async responses — rapidly changing the selected track could let a stale response overwrite the vocal ratio (and therefore the player's accent color) for whatever is actually selected by the time it resolves. Fixed by adding a monotonic request-id guard, mirroring the identical, already-existing pattern used for waveform peaks (`peakRequestId`). No cargo/tsc changes required beyond the guard itself; `tsc --noEmit` is clean.
- **Graceful degradation confirmed by direct code reading, not assumed:** `detect_vocal_ratio` (`crates/audio-analysis/src/lib.rs:245-283`) returns `None` at every failure point — clip too short, `VoiceActivityDetector::builder().build().ok()?` failing (i.e. the ONNX runtime not loading), zero usable chunks. Nothing panics or propagates an error that could crash playback. Worst case on a broken environment: the player quietly falls back to its tag-only mood guess.
- **Real risk, not yet exercised:** `voice_activity_detector` pulls in `ort`/`ort-sys`, which downloads a prebuilt ONNX Runtime binary over the network the first time `cargo build`/`cargo test` touches `audio-analysis` (see `Cargo.lock`). This dependency was added after the last push to `origin/main` — CI's `windows-latest` job has never actually built it. **This is the single most important thing to check before or alongside the Windows trial** (see the pre-flight checklist at the bottom of this document).
- **Packaging not yet solved, but not a regression:** there's no `bundle.resources` entry for the ONNX Runtime shared library, and `tauri.conf.json` still has `bundle.active: false` — consistent with the fact that no installer pipeline exists for anything yet (Milestone 7). Tracked in ADR 0027 as a Milestone 7 prerequisite alongside the existing `similarity-worker` sidecar bundling problem, not fixed here.
- **Scope is intentionally narrow:** `vocal_ratio` only feeds the player's mood color today. It's stored per-asset and easy to surface further (a real "quick win," see Missing Features below).

---

## 1. Core Workflow

| Item | Status | Notes |
|---|---|---|
| Import folders/packs | Complete | File-picker folder import; `docs/genesis/...` §19 |
| Automatic cataloguing | Complete | |
| Keep originals on NAS/external | Complete | Referenced/hybrid library mode; any path incl. SMB mount |
| DB stored locally only | Complete | SQLite catalog is always local, per ADR 0002/0007 |
| Browse while NAS offline | Complete | Availability state tracked; catalog stays queryable |
| Reconnect automatically | Partial | Reconnect **validation** is real and manifest-scoped (ADR 0022); it runs on demand, not a silent background auto-poll the moment the mount reappears |
| Drag files/folders onto the app to import | **Missing** | Only file-picker import exists; no window drop-target wired |
| Watched Downloads folder | Partial | Real standing poller (ADR 0026), but one folder → one library only, ~20s cadence, no per-file progress UI |

## 2. Automatic Intelligence

| Item | Status | Notes |
|---|---|---|
| Metadata | Complete | |
| Waveform | Partial | Transport waveform is real; per-row waveform is a static icon, not cached peak data (ADR 0020) |
| Preview cache | **Missing** | No low-bitrate offline-audition cache (§10.5) |
| Duplicate detection | Partial | Exact content-hash dupes: Complete, with a real "keep oldest, trash rest" action. Equivalent/related-variant tiers (§13) aren't a dedicated duplicate-manager feature — similarity search partially substitutes |
| Filename intelligence | Complete | |
| AI tag suggestions | Complete | Rule-based, confidence + origin tracked (`TagOrigin`) |
| Category detection | Complete | |
| Mood | Partial | Only as a coarse player-mood classifier (soundtrack/voice/SFX), not a full mood tag facet |
| Genre | Partial | Embedded-metadata genre extraction only; no audio-classified genre |
| Instrument detection | **Missing** | |
| Ambience detection | **Missing** | "Ambience" exists as a top-level media type, not as an audio-derived classifier |
| BPM | Complete | Hand-written autocorrelation estimator, ADR 0025 |
| Musical key | Partial | Populates the column via a **monophonic pitch/note estimate**, explicitly labeled "detected pitch" in the UI, not real polyphonic key detection — an honest, deliberate limitation (ADR 0025) |
| Duration | Complete | |
| Loudness | Partial | `peak_db` real; `loudness_lufs` explicitly unpopulated (ADR 0025) |
| Stereo/mono | Partial | Channel count captured; no dedicated stereo-width measurement |
| Sample rate | Complete | |
| Bit depth | Partial | Column exists, unpopulated — decoded f32 samples don't carry original bit depth (ADR 0025) |

**Manual work after import:** review/accept AI tag suggestions, correct mood/category misfires, confirm musical key if it matters (it's a pitch guess), fill in source/license. This is already close to the spec's "improves progressively, no wizard" goal (§3.5) — reasonable for a v1.

## 3. Library Organization

| Item | Status | Notes |
|---|---|---|
| AI suggested tags, one-click approval | Complete | |
| Batch editing | Complete | Tag/favorite/export/trash all bulk-capable |
| Drag onto tags/collections | **Missing** | Click-to-apply only; `workspace-state`'s `DragPayload`/`DragTarget` types exist and are tested but nothing in the frontend dispatches an HTML5 drag |
| Smart Collections | Complete | Live-evaluated (ADR 0026) |
| Manual Collections / Projects | Complete | |
| Favorites | Complete | |
| Ratings | **Missing** | No rating field anywhere in the data model or UI |
| Recently imported | Complete | |
| Recently used (played) | Partial | `last_played` is a real, populated storage column (`crates/storage/src/lib.rs:1662`) with **no** sidebar filter or sort surfacing it |
| Never used | **Missing** | No filter for zero `play_count`/`export_count` |
| Missing files | Complete | Filter + a direct "Relink…" action (ADR 0026) |
| Duplicate manager | Partial | Exact duplicates only (see §2) |

## 4. Search Experience

| Item | Status |
|---|---|
| Keywords (FTS) | Complete |
| AI tags | Complete |
| Mood / Genre / Instrument | **Missing** as dedicated filters (tag-search covers user-applied mood/genre tags loosely) |
| Category (media type) | Complete |
| BPM range | Complete (ADR 0026) |
| Musical key | **Missing**, deliberate — not a numerically meaningful range (per plan's own annotation) |
| Duration range | Complete |
| Source / License | **Missing** |
| Project used | Partial — browsable via the project's own nav item, not a search-bar filter chip |
| Rating / Favorites | Favorites: Complete. Rating: Missing (no field) |
| Date imported / last used / times used | **Missing** |
| File format | **Missing** |

Is search instant? Functionally yes for the FTS5-backed query at small/medium scale. A 100,000-asset profiling test exists (`large_catalog_search_profile_exercises_one_hundred_thousand_assets`) but is `#[ignore]`d and not run in CI — "instant at scale" is unverified, not disproven.

## 5. Audio Browser

| Item | Status |
|---|---|
| Arrow-key row navigation | **Missing** — confirmed by direct grep: only `ArrowLeft`/`ArrowRight` exist (`App.tsx:2297-2300`), and they seek within the current track, not move row focus |
| Instant next-sound audition | Partial — real once a row is clicked; not reachable by arrow keys since row navigation itself doesn't exist |
| Continue playback while browsing | Complete | Transport is a persistent, floating element |
| Waveforms | Partial | See §2 |
| Scrub | Complete | Click/drag-to-seek (ADR 0023/0026) |
| Loop | Partial | Real toggle button; no `L` keyboard shortcut (spec §6.3) |
| In/out points | **Missing** | No `I`/`O` marking, no preview-range UI |
| Playback speed | **Missing** | Confirmed zero references anywhere in the frontend |
| Normalize preview volume | **Missing** | No loudness-based playback gain |

## 6. Editor Workflow

| Item | Status |
|---|---|
| Drag into Premiere/Resolve/Final Cut/AE/Finder | **Missing** | Zero `draggable`/`dragstart` usage anywhere in `App.tsx` |
| Copy file path | **Missing** | Zero clipboard usage anywhere — not even a button |
| Reveal original file / open containing folder | **Missing** | `tauri-plugin-opener` is installed and registered (`lib.rs:1805`) — the cross-platform-safe capability is one call away — but nothing in the frontend invokes it |
| Copy to project folder | Complete | |
| DaVinci Resolve quick-export | Complete | New this pass, see §0 |

This is the weakest section relative to spec intent. The "editor's dream" quick-export (DR button) is a real, good substitute for one specific NLE, but the universal drag-out/copy-path/reveal trio that the plan calls "first release priority" (§6.5) is entirely unbuilt, despite the enabling plugin already being a dependency.

## 7. Project Intelligence

| Item | Status |
|---|---|
| Which project used each sound | Complete | `UsageEvent`, keyed by `asset_id` + `project_id` |
| Last usage / number of uses | Partial | Data is captured and feeds the license report CSV; no dedicated usage-history view |
| User notes | **Missing** | `notes` exists in the §17.1 data model on paper; zero references to a notes field anywhere in `App.tsx` — not exposed at all |
| Custom collections per project | Complete | Projects are just Collections with `type: Project` |

## 8. Performance

| Item | Status |
|---|---|
| Instant scrolling | **Missing** | No row virtualization — the UI's own "Not yet virtualized" label is literal, not modesty |
| Instant search | Partial | See §4 |
| Immediate playback | Complete | Native `<audio>` via Tauri asset protocol |
| Smooth waveform rendering | Partial | Works, but recomputed client-side each load, not cached |
| Background indexing | Complete | Real priority queue, atomic claiming, bounded retry, standing worker (ADR 0026) |
| Lazy loading | **Missing** | Tied to the virtualization gap |
| Cache management | Partial | No waveform-peak disk cache, no offline preview cache; job/backup caching is otherwise solid |
| Low RAM usage | Not verified | Memory scales with row count with no virtualization in place — real risk, untested |
| 500k+ assets | Not verified, likely at risk | The project's own ignored profiling test targets 100k, not 500k; virtualization absence is the dominant risk factor here, repeatedly flagged by the plan's own status notes |

## 9. Design

| Item | Status |
|---|---|
| Native typography | Complete | System font stack |
| Apple-style hierarchy / Liquid Glass | Complete | Extensively rebuilt this session — foldable glass cards, floating rounded side panels |
| Smooth animations | Complete | `motion` (Motion for React) is genuinely imported and used (`App.tsx:3`), not a dead dependency |
| Waveform animation | Complete | Gradient fill + pulse animation on the active transport waveform |
| Responsive layout | Partial | Collapsible panels work well; the shell assumes a desktop window (`min-width: 920px`) rather than fluid responsive breakpoints — reasonable for this product category, worth naming honestly |
| Modern spacing | Complete | |
| Keyboard-first workflow | Partial | Many real shortcuts (play/favorite/search-focus/import/export); arrow-row-nav, loop, in/out, quick-tag-by-number, speed are all missing |
| Accessibility | Partial | Reduced motion + reduced transparency are real, persisted toggles. `aria-label` is used broadly (41 instances). High-contrast mode and a real accessibility audit are both **Missing/Not started**, matching the plan's own Milestone 7 caveat |
| Dark Mode | Complete | |
| Light Mode | **Missing** | Zero `prefers-color-scheme: light` or theme toggle anywhere. Worth naming plainly: the original spec calls for "dark-first but fully supports light mode" (§15.1) — that promise isn't kept. This matches this session's own explicit direction ("keep dark tones as standard"), so it may be a deliberate product choice rather than an oversight — flagging so it's a decision, not an accident |

## 10. Storage Architecture

The plan's architecture (local SQLite catalog, NAS/external for media only, no live DB over the network share) is followed correctly and consistently — this is a genuine strength, not just a checkbox. Specifically:

- **TrueNAS/shared storage:** Complete — referenced/hybrid mode works with any mounted path.
- **Local machine data:** Complete for catalog + preferences; **Partial** for "search index" (it's the same SQLite FTS5 table, which is the plan's own recommended MVP approach, not a separate index — correct, not a gap); **Missing** for a persisted waveform/preview cache.
- **No live DB on NAS:** correctly avoided — confirmed by reading the architecture, not just the docs.

**Real risks found:**
- The portable library manifest (§9.3) is built in-memory on demand (for backup and reconnect-validation) rather than continuously maintained beside the media. On a lost/corrupted local `catalog.sqlite`, there is no independent, NAS-side recovery record of tags/collections/source-license data — only the last local backup. Worth treating as a genuine data-durability gap, not a nice-to-have.
- No writer-lease enforcement across machines sharing one NAS media root (`acquire_lease_file`/`release_lease_file` exist, tested, uncalled — ADR 0022). Two people/machines pointed at the same NAS folder today have no reconciliation or read-only warning between their independent local catalogs. The planned `nas/multi-computer-use.md` doc page was never written either — this is an honest gap, not just a doc gap.

## 11. Missing Features — what I'd add myself

Scoped to: improves editor workflow, reduces repetitive work, increases speed, improves organization, or differentiates. Ordered roughly cheapest/highest-leverage first, not by section:

1. **Copy file path + Reveal in Finder/Explorer.** The enabling plugin (`tauri-plugin-opener`) is already a dependency and already registered — this is a near-zero-effort, high-frequency-use feature that's currently just... not called.
2. **Playback speed control.** The native `<audio>` element already supports `playbackRate` for free — no engine change needed. High value for reviewing long dialogue/ambience quickly.
3. **Surface `vocal_ratio` beyond player color** — an inspector pill ("Vocal" / "Instrumental" / "Mixed") and a matching search filter. The hard part (real detection) is already done; this is pure UI wiring on data that already exists.
4. **Rating + "Never used" smart filter.** Both are small, well-precedented additions (Favorites and Missing-Files filters already establish the pattern).
5. **Arrow-key row navigation.** The single biggest gap relative to the stated "Artlist-like" browsing goal — the app currently cannot be driven top-to-bottom by keyboard at all, which directly contradicts Principle 4 ("keyboard use must be as complete as mouse use").
6. **Command-palette frontend (Cmd/Ctrl+K).** The registry already exists and is tested (`command-palette` crate, ADR 0013) with exactly one trivial Tauri command consuming it (`default_command_titles`, `lib.rs:293-300`) and zero frontend usage. A real palette UI is most of a "professional tool" feel for comparatively little backend work.
7. **Persisted waveform peak cache.** Removes a real, repeated client-side decode cost and is explicitly what §10.4 calls for.
8. **Remaining §11.2 search filters** (source, license, date added, last used, times used, file format) — the underlying data already exists in every case; this is query/UI work, not new data collection.
9. **Row virtualization.** Not optional if "large libraries" is a real commercial claim — this is the one item on this list I'd treat as a release blocker rather than a nice-to-have, given how many other "Not verified at scale" notes trace back to it.
10. **OS-level drag-and-drop**, both directions (drop files onto the window to import; drag a row out to Finder/Explorer/an NLE). Larger effort (real HTML5/Tauri drag plumbing), but it's the one item repeatedly named as "first release priority" in the plan itself that never happened.

Deliberately **not** recommending: instrument/ambience ML classifiers, a full 3-tier duplicate manager, light mode, or a writer-lease/multi-computer sync layer — all real spec items, but each is a substantial build with a smaller near-term payoff than the list above. They belong in a post-V1 roadmap, not this one.

---

## UX Improvements

- Keyboard-drive the browser (arrow keys) before anything else in this category — every other keyboard shortcut in the app is undermined by not being able to move through the list without a mouse.
- Add the copy-path/reveal pair to the transport and/or a row context menu — cheap, and closes the single biggest gap in "get this sound into my editor" flow.
- Show `last_played` and a "Never used" state in the sidebar — the data already exists and is currently invisible.
- Consider a lightweight per-asset Notes field in the inspector — spec'd, zero-cost relative to the rest of this list, and directly useful for the "why did I keep this" problem the whole product exists to solve.

## Technical Improvements

- Persist waveform peaks to disk instead of recomputing per load (§10.4) — the highest-value performance fix that doesn't require virtualization.
- Add row virtualization to the browser — the current honest "Not yet virtualized" label is doing its job; it's time to close it.
- Wire a real background reconnect poll for NAS availability rather than relying on on-demand validation.
- Run the existing `#[ignore]`d 100k-asset search-profiling test in CI on a schedule (not every push) so "search stays fast at scale" stops being an assumption.

## Architecture Improvements

- Solve ONNX Runtime + `similarity-worker` sidecar bundling together as one Milestone-7 packaging task before `bundle.active` flips to `true` — both need the same kind of `bundle.resources`/`externalBin` treatment and are cheaper to solve as one pass than two.
- Continuously maintain the portable library manifest beside the media (§9.3) instead of building it on demand, and wire the already-built, already-tested writer-lease functions to it — this is the real remaining piece of the "multi-computer/NAS" story, not a documentation gap.
- Decide deliberately on light mode: either commit to it (real spec requirement, §15.1) or formally amend the spec to "dark-only," so it stops being a silent gap.

## Priority Order for Implementation

**P0 — before/with the Windows trial (verification, not development):**
1. Push this branch and check the `windows-latest` CI run — first real signal on whether `ort`/`voice_activity_detector` builds on Windows at all.
2. Build a Windows-triple `similarity-worker` sidecar (`scripts/build-similarity-worker-sidecar.sh`, run on the Windows machine itself) before the first `tauri dev`/`tauri build` there — without it, Tauri won't even launch.

**P1 — cheap, high-value:**
3. Copy file path + Reveal in Finder/Explorer.
4. Playback speed control.
5. Surface `vocal_ratio` as an inspector value + filter.
6. Rating field + Never-used filter.

**P2 — moderate effort, core spec fidelity:**
7. Arrow-key row navigation.
8. Command-palette frontend.
9. Remaining §11.2 search filters.
10. Persisted waveform peak cache.

**P3 — larger infrastructure:**
11. Row virtualization.
12. OS-level drag-and-drop (both directions).
13. Offline preview cache.
14. Continuously-maintained portable manifest + writer-lease wiring.

**P4 — release readiness (deliberately last, matches the project's own stated posture):**
15. Real accessibility/performance/crash-recovery audits, code signing/notarization, auto-update, a considered light-mode decision, platform-specific (Windows) design pass.

## Updated Roadmap

This document fills the `product/roadmap.md` slot the plan's own documentation structure (§23) always intended but never populated. Recommended framing going forward: Milestones 0–6 are substantially real and don't need re-litigating; Milestone 7 ("Product polish and release readiness") is the actual remaining gate between "feature-complete MVP" and "V1," and P0–P3 above are best treated as a **Milestone 6.5** — the highest-leverage gaps in the already-shipped milestones — to close before spending effort on Milestone 7's audits and signing work, since several of those audits (accessibility, performance) are more meaningful once P2/P3 items land.

---

## Windows Trial Pre-Flight Checklist

Specific to "try the application on a Windows machine" being next:

1. **Push to `origin/main`** (done as part of this review) and watch the `windows-latest` job in `ci.yml` — it runs `cargo test --workspace`, which is the first time the new `ort`/`voice_activity_detector` dependency will be built on Windows at all.
2. **On the Windows machine, before the first `tauri dev`/`tauri build`:** install Rust + run `scripts/build-similarity-worker-sidecar.sh` to produce the Windows-triple `similarity-worker` binary Tauri's `externalBin` config requires — without this, the desktop shell won't build/launch, independent of anything else in this report.
3. Expect **no crash** if the ONNX runtime fails to load on Windows — `detect_vocal_ratio` degrades to `None` and the player falls back to tag-only mood coloring (verified by code reading, see §0). If vocal-based coloring silently never appears on Windows, that confirms the ONNX runtime didn't load — check the CI run from step 1 first.
4. Everything else audited in this review (missing drag-and-drop, no virtualization, etc.) is platform-agnostic — expect the same gaps on Windows as on macOS, not new ones, apart from the two build-time items above.
