---
title: V1 Readiness Review
description: Full spec-vs-implementation audit ahead of the first Windows trial and a V1 decision.
---

_Reviewed 2026-07-31, against `docs/genesis/audio_library_application_plan.md` (updated the same day), ADRs 0001–0027, and direct inspection of the current codebase. This is a review, not new development — nothing described as "Missing" below was built as part of this pass, except two small hardening fixes called out explicitly in Section 0._

**Update, same day:** the four highest-leverage items from this review were then built in a follow-up development pass — the Editor Workflow panel (§6), the command palette (§5/§9), light mode (§9), and row virtualization (§8). Sections below are marked accordingly; the original review text is otherwise left as written so the "before" state stays visible. One finding was corrected in the process: the original §5 claim that arrow-key row navigation was missing was wrong (it was already wired via `playRelative`) — see the note in §5.

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
| Arrow-key row navigation | **Correction, not a fix — Complete already.** This review originally said "Missing," based on grepping the frontend for literal `"ArrowUp"`/`"ArrowDown"` strings. Wrong — those keys are wired through `crates/preferences`' data-driven shortcut table (`ArrowDown`→`NextAsset`, `ArrowUp`→`PreviousAsset`), resolved in `App.tsx`'s `handleKeyDown`, and drive `playRelative`, which moves selection and plays. Nothing needed building here. |
| Instant next-sound audition | Complete — real via click and via Up/Down (see above) |
| Continue playback while browsing | Complete — transport is a persistent, floating element |
| Waveforms | Partial — see §2 |
| Scrub | Complete — click/drag-to-seek (ADR 0023/0026); Shift+Arrow now gives a larger seek step, per spec §6.3 |
| Loop | **Complete (built).** Real toggle button, now also a real `L` keyboard shortcut (`ToggleLoop` in `crates/preferences`) |
| In/out points | **Missing** — no `I`/`O` marking, no preview-range UI |
| Playback speed | **Missing** — confirmed zero references anywhere in the frontend |
| Normalize preview volume | **Missing** — no loudness-based playback gain |

## 6. Editor Workflow

**Update:** the three items below marked "Complete (built)" were built in the Editor Workflow panel — a new animated section, opened from a sidebar button, consolidating all of this app's editor-integration actions in one place (`App.tsx`'s `editor-workflow` section; `Reveal`/`Copy File Path`/`Send to Project` actions). True OS-level drag-and-drop was investigated and deliberately not built — see the note below.

| Item | Status |
|---|---|
| Drag into Premiere/Resolve/Final Cut/AE/Finder | **Missing, deliberately.** Investigated directly: Tauri has no first-party support for dragging a file out of the webview, and the only plugin found is a small, unofficial, macOS-only project. Confirmed with the product owner that the watched-folder + quick-export flow below already covers this need |
| Copy file path | **Complete (built).** `tauri-plugin-clipboard-manager`, wired to the Editor Workflow panel plus a real `Mod+Shift+C` shortcut |
| Reveal original file / open containing folder | **Complete (built).** `tauri-plugin-opener`'s `revealItemInDir`, cross-platform, wired to the Editor Workflow panel |
| Copy to project folder | Complete |
| DaVinci Resolve quick-export | Complete — now also has a dedicated entry point in the Editor Workflow panel, alongside the existing player/sidebar buttons |

This was the weakest section relative to spec intent at review time. The universal copy-path/reveal pair is now real and cross-platform; literal drag-and-drop remains the one deliberately-unbuilt item, by explicit product decision rather than oversight.

## 7. Project Intelligence

| Item | Status |
|---|---|
| Which project used each sound | Complete — `UsageEvent`, keyed by `asset_id` + `project_id` |
| Last usage / number of uses | Partial — data is captured and feeds the license report CSV; no dedicated usage-history view |
| User notes | **Missing** — `notes` exists in the §17.1 data model on paper; zero references to a notes field anywhere in `App.tsx` — not exposed at all |
| Custom collections per project | Complete — projects are just Collections with `type: Project` |

## 8. Performance

**Update:** row virtualization is now real (`computeVisibleRowRange` in `App.tsx`, a verified port of `crates/viewport::VirtualViewport::visible_range`) — this was the dominant risk factor behind most of this section's "Not verified" ratings, so several are upgraded below.

| Item | Status |
|---|---|
| Instant scrolling | **Complete (built).** Only the scrolled window plus overscan is ever mounted, independent of library size; the old "Not yet virtualized" label is gone because it's no longer true |
| Instant search | Partial — see §4 |
| Immediate playback | Complete — native `<audio>` via Tauri asset protocol |
| Smooth waveform rendering | Partial — works, but recomputed client-side each load, not cached |
| Background indexing | Complete — real priority queue, atomic claiming, bounded retry, standing worker (ADR 0026) |
| Lazy loading | **Complete (built)** — see Instant scrolling above |
| Cache management | Partial — no waveform-peak disk cache, no offline preview cache; job/backup caching is otherwise solid |
| Low RAM usage | Improved, not fully verified — DOM node count is now bounded regardless of row count; underlying data structures (e.g. `selected_indices` as an array) still scale with selection size, untested at very large selections |
| 500k+ assets | Improved, not fully verified — the dominant risk (virtualization) is closed; the project's own ignored profiling test still only targets 100k, not 500k, and remains unverified in CI |

## 9. Design

**Update:** light mode and the command palette were both built (see §0's follow-up note and §5/§6 above for the palette and Editor Workflow respectively).

| Item | Status |
|---|---|
| Native typography | Complete — system font stack |
| Apple-style hierarchy / Liquid Glass | Complete — extensively rebuilt this session — foldable glass cards, floating rounded side panels |
| Smooth animations | Complete — `motion` (Motion for React) is genuinely imported and used (`App.tsx:3`), not a dead dependency |
| Waveform animation | Complete — gradient fill + pulse animation on the active transport waveform |
| Responsive layout | Partial — collapsible panels work well; the shell assumes a desktop window (`min-width: 920px`) rather than fluid responsive breakpoints — reasonable for this product category, worth naming honestly |
| Modern spacing | Complete |
| Keyboard-first workflow | Improved — play/favorite/search-focus/import/export/loop/copy-path/row-navigation/command-palette are all real; `1`–`9` quick-tags and `I`/`O` in/out marking remain unbuilt |
| Accessibility | Partial — reduced motion + reduced transparency are real, persisted toggles. `aria-label` is used broadly. High-contrast mode and a real accessibility audit are both **Missing/Not started**, matching the plan's own Milestone 7 caveat |
| Dark Mode | Complete |
| Light Mode | **Complete (built).** CSS custom properties (`--text-primary`, `--bg-app`, `--surface-*`) with a `:root[data-theme="light"]` override, using the provided Silver Mist / Flameburst Orange / Midnight Shadow palette. Dark remains the default per this session's own earlier direction — light and "match system" are new, real, persisted options (`ThemePreference` in `crates/preferences`), not a replacement |

## 10. Storage Architecture

The plan's architecture (local SQLite catalog, NAS/external for media only, no live DB over the network share) is followed correctly and consistently — this is a genuine strength, not just a checkbox. Specifically:

- **TrueNAS/shared storage:** Complete — referenced/hybrid mode works with any mounted path.
- **Local machine data:** Complete for catalog + preferences; **Partial** for "search index" (it's the same SQLite FTS5 table, which is the plan's own recommended MVP approach, not a separate index — correct, not a gap); **Missing** for a persisted waveform/preview cache.
- **No live DB on NAS:** correctly avoided — confirmed by reading the architecture, not just the docs.

**Real risks found:**
- The portable library manifest (§9.3) is built in-memory on demand (for backup and reconnect-validation) rather than continuously maintained beside the media. On a lost/corrupted local `catalog.sqlite`, there is no independent, NAS-side recovery record of tags/collections/source-license data — only the last local backup. Worth treating as a genuine data-durability gap, not a nice-to-have.
- No writer-lease enforcement across machines sharing one NAS media root (`acquire_lease_file`/`release_lease_file` exist, tested, uncalled — ADR 0022). Two people/machines pointed at the same NAS folder today have no reconciliation or read-only warning between their independent local catalogs. The planned `nas/multi-computer-use.md` doc page was never written either — this is an honest gap, not just a doc gap.

## 11. Missing Features — what I'd add myself

Scoped to: improves editor workflow, reduces repetitive work, increases speed, improves organization, or differentiates. Ordered roughly cheapest/highest-leverage first, not by section.

**Built since this review:** copy file path + reveal in Finder/Explorer, arrow-key row navigation (turned out to already exist), a command-palette frontend, and row virtualization — items 1, 5, 6, and 9 from the original numbered list. What remains:

1. **Playback speed control.** The native `<audio>` element already supports `playbackRate` for free — no engine change needed. High value for reviewing long dialogue/ambience quickly.
2. **Surface `vocal_ratio` beyond player color** — an inspector pill ("Vocal" / "Instrumental" / "Mixed") and a matching search filter. The hard part (real detection) is already done; this is pure UI wiring on data that already exists.
3. **Rating + "Never used" smart filter.** Both are small, well-precedented additions (Favorites and Missing-Files filters already establish the pattern).
4. **Persisted waveform peak cache.** Removes a real, repeated client-side decode cost and is explicitly what §10.4 calls for.
5. **Remaining §11.2 search filters** (source, license, date added, last used, times used, file format) — the underlying data already exists in every case; this is query/UI work, not new data collection.
6. **OS-level drag-and-drop for importing** (dropping files/folders onto the app window). Note this is distinct from dragging *out* to other apps, which was investigated and deliberately not built (§6) — the product owner confirmed the watched-folder + quick-export flow already covers that direction.

Deliberately **not** recommending: instrument/ambience ML classifiers, a full 3-tier duplicate manager, or a writer-lease/multi-computer sync layer — all real spec items, but each is a substantial build with a smaller near-term payoff than the list above. They belong in a post-V1 roadmap, not this one.

---

## UX Improvements

- ~~Keyboard-drive the browser (arrow keys)~~ — already worked; corrected in this review, not built.
- ~~Add the copy-path/reveal pair to the transport and/or a row context menu~~ — **built**, in the new Editor Workflow panel.
- Show `last_played` and a "Never used" state in the sidebar — the data already exists and is currently invisible.
- Consider a lightweight per-asset Notes field in the inspector — spec'd, zero-cost relative to the rest of this list, and directly useful for the "why did I keep this" problem the whole product exists to solve.

## Technical Improvements

- Persist waveform peaks to disk instead of recomputing per load (§10.4) — the highest-value remaining performance fix.
- ~~Add row virtualization to the browser~~ — **built.**
- Wire a real background reconnect poll for NAS availability rather than relying on on-demand validation.
- Run the existing `#[ignore]`d 100k-asset search-profiling test in CI on a schedule (not every push) so "search stays fast at scale" stops being an assumption.

## Architecture Improvements

- Solve ONNX Runtime + `similarity-worker` sidecar bundling together as one Milestone-7 packaging task before `bundle.active` flips to `true` — both need the same kind of `bundle.resources`/`externalBin` treatment and are cheaper to solve as one pass than two.
- Continuously maintain the portable library manifest beside the media (§9.3) instead of building it on demand, and wire the already-built, already-tested writer-lease functions to it — this is the real remaining piece of the "multi-computer/NAS" story, not a documentation gap.
- ~~Decide deliberately on light mode~~ — **decided and built**: dark stays the default, light and system are real options.

## Priority Order for Implementation

**P0 — before/with the Windows trial (verification, not development):**
1. Push this branch and check the `windows-latest` CI run — first real signal on whether `ort`/`voice_activity_detector` builds on Windows at all.
2. Build a Windows-triple `similarity-worker` sidecar (`scripts/build-similarity-worker-sidecar.sh`, run on the Windows machine itself) before the first `tauri dev`/`tauri build` there — without it, Tauri won't even launch.

**Built since this review (formerly P1/P2/P3 items 3, 7, 8, 11):** copy file path + reveal in Finder/Explorer, arrow-key row navigation (already existed), command-palette frontend, and row virtualization.

**P1 — cheap, high-value, still open:**
3. Playback speed control.
4. Surface `vocal_ratio` as an inspector value + filter.
5. Rating field + Never-used filter.

**P2 — moderate effort, core spec fidelity, still open:**
6. Remaining §11.2 search filters.
7. Persisted waveform peak cache.

**P3 — larger infrastructure, still open:**
8. OS-level drag-and-drop for *importing* (dropping files onto the window) — exporting via drag was deliberately descoped, see §6.
9. Offline preview cache.
10. Continuously-maintained portable manifest + writer-lease wiring.

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
4. Everything else audited in this review (e.g. deliberately-descoped OS-level drag-out) is platform-agnostic — expect the same gaps on Windows as on macOS, not new ones, apart from the two build-time items above.
