# ADR 0026: Finalization pass — playback polish, faceted search, job system maturity, NAS/export gaps

## Status

Accepted.

## Context

`docs/genesis/audio_library_application_plan.md`'s Sections 19-20 track every
deliverable's real status (`[Done]`/`[Partial]`/`[Not started]`). This pass
picked up four of the tracked gap clusters — playback polish, organization/
search completeness, the background job system's maturity, and NAS/export
gaps — after research turned up more precise (and in one case, materially
different) scope than the milestone doc's one-line descriptions suggested.

Two things changed the plan mid-research:

**A platform blocker ruled out output-device selection.** `output_device`
was already stored/persisted but only ever displayed read-only. Applying it
requires `HTMLMediaElement.setSinkId()`, which WKWebView (macOS's webview,
what Tauri uses here) does not implement at all — Chromium-based webviews
(Windows) are the only place it would work. Building a device picker for a
control that's silently inert on the platform doing the building would be
worse than leaving it alone; excluded, with a one-line explanation added to
Settings instead of a non-functional control.

**A real correctness bug surfaced while researching the job system.**
`process_audio_analysis_jobs` reads pending jobs, then processes them across
several separate short mutex holds — correct per ADR 0023/0024's lesson
about never holding the catalog mutex across slow work, but it meant two
overlapping calls could both select and process the *same* job before either
marked it done. This was latent before this pass (the job system's three
trigger points rarely overlapped), but adding a standing background worker
(this pass's other job-system change) makes overlap routine — so the race
got fixed as a prerequisite, not an afterthought.

## Decision

### Playback: loop + keyboard seek

`<audio loop={looping}>` plus a transport toggle button. Setting the native
`loop` attribute means the `ended` event never fires while looping, so the
existing `onEnded={() => playRelative(1)}` auto-advance-to-next-track
behavior is automatically superseded by single-track looping when enabled —
no extra branching needed.

The transport's seek slider already had `role="slider"`/`tabIndex`/
`aria-value*` from an earlier pass but no keyboard handler — added
ArrowLeft/ArrowRight nudging, completing accessibility semantics that were
half-built rather than introducing a new interaction pattern.

### Job system: atomic claiming, bounded retry, a standing worker

**Atomic claiming.** `claim_pending_jobs(kind, limit)` replaces the
plain-SELECT `pending_jobs_of_kind` at both job-processing command call
sites. One SQL statement does the select-and-mark in one step:

```sql
UPDATE background_jobs SET state = 'processing', updated_at = ?1
WHERE id IN (
  SELECT id FROM background_jobs WHERE state = 'pending' AND kind = ?2
  ORDER BY priority ASC, created_at ASC LIMIT ?3
)
RETURNING id, asset_id, kind, priority
```

No explicit transaction wrapper needed — a single statement is atomic on its
own, and the catalog mutex is already held for this one step. `RETURNING`
needs SQLite 3.35+; `rusqlite`'s `bundled` feature ships a recent enough
version. A job stuck in `'processing'` after a crash mid-batch is an
accepted edge case — no lease-timeout/reclaim mechanism, to keep this
bounded.

**Bounded retry.** `fail_job` previously abandoned a job permanently after
one attempt (attempts were written, never read back, nothing ever
requeued). `requeue_failed_jobs(max_attempts)` — called once per worker
tick with a cap of 3 — moves `'failed'` jobs with attempts under the cap
back to `'pending'`, giving transient failures (a momentarily unreachable
NAS path, say) a real gap before retrying rather than retrying immediately
inline.

**Standing worker.** A single `std::thread::spawn` loop started in `.setup()`
(not async/tokio — each tick's work is a handful of synchronous SQL
statements and a directory read, nothing here benefits from an async
runtime), ticking every ~20s: requeue failed jobs, poll the configured
watched folder if one's set, emit `background-tick`. The frontend listens
and calls the already-existing `runJobDrain` — the worker *triggers*
draining, it doesn't reimplement the draining logic that already existed
(and already correctly spawns the GPL-isolated similarity-worker subprocess,
handles decode, etc.) in the frontend/Tauri-command layer.

This is what actually fixes "jobs only process right after Import/Refresh"
— for real, continuously, not just at three explicit trigger points.

### Faceted filters and Smart Collections turned out to be the same feature

`create_smart_collection` already existed and stored a serialized
`AssetSearchQuery` — but `AssetSearchQuery` only had `text`/`tag_id`/
`media_type` (no numeric ranges), and nothing ever evaluated a stored smart
collection back into results. `assets_in_collection` had no branch for
`CollectionType::Smart` at all; calling it against a smart collection would
just return an empty static-membership list forever.

`AssetSearchQuery` gained `duration_min_ms`/`duration_max_ms`, `bpm_min`/
`bpm_max`, `peak_db_min`/`peak_db_max` (peak_db as the "energy" facet — a
real persisted column already; a numeric range on `musical_key` wouldn't be
meaningful, so pitch has no range filter). `search_assets` grew matching
optional SQL clauses. `assets_in_smart_collection(collection_id)` is the
missing "evaluate" half: load the collection, deserialize its stored query,
re-run it through `search_assets`.

Given that, building a separate smart-collection query-builder UI would have
been redundant — the range-filter panel already built for ad-hoc search
*is* the query builder. "Save as Smart Collection" just snapshots the
current search text + range filters into `create_smart_collection`. Smart
collections show up in the same sidebar Projects list as manual collections
(same `CollectionRecord`/`collections` table), marked with a small icon;
selecting one calls `assets_in_smart_collection` instead of
`assets_in_collection`. The "add selected assets to project" quick-action
grid excludes smart collections — their membership is computed, not stored,
so manually adding to one would silently write rows nothing ever reads.

### NAS, export, and watched folders

**Missing-file relink.** A thin command wrapping the already-existing
`storage::relink_asset` (took a path and flipped availability state, had no
caller). A "Relink…" button appears per-row only when
`availability_state === "Missing"`, opening a file picker and calling the
new command — first real caller of code that's existed since ADR 0022.

**WAV conversion export.** `export_selected_asset` was hardcoded to
`ExportPreset::Original`. It now accepts an optional `format`; when it's
the WAV preset, the source gets decoded via
`audio_metadata::decode_any_supported_audio` (the same Symphonia-backed seam
ADR 0025 built) and rendered via `export_pipeline::render_wav_export`
(fully implemented and tested since an earlier pass, never called outside
its own test suite until now). A small format dropdown sits next to the
existing Export Selected button.

**Watched folder.** `import_pipeline::WatchedFolderPoller` — size-
stabilization polling logic, tested since an early milestone, never
connected to the running app — is now owned by the standing worker across
ticks (its internal state needs to persist between polls, which a
long-lived thread closure provides for free) and its discovered files
import via `import_pipeline::import_file`, the same per-file function
`import_folder` already loops over (so the full pipeline — job enqueueing,
tag suggestions, everything — runs identically, not a parallel simplified
path). One folder, not a list, matching the plan doc's own "Watched
Downloads folder" (singular). A new `watched_folder_path` +
`watched_folder_library_id` preference pair (global preferences are
per-app, not per-library, so watching needs an explicit library to import
into — chosen alongside the folder in Settings).

## Consequences

- `AssetSearchQuery` and `AssetRecord` both dropped `Eq` from their derives
  when gaining `f64` fields (floats can't implement `Eq`) — `PartialEq`
  remains, which is what every actual usage (assertions, comparisons)
  needed.
- The standing worker's watched-folder import runs with no user-visible
  progress beyond the next `background-tick`-triggered refresh — acceptable
  at a 20s tick cadence for a background convenience feature, not something
  this pass added dedicated UI for.
- `output_device` remains stored and displayed but not enforced, with that
  limitation now stated in Settings rather than implied by a picker that
  would do nothing.
- Pause/resume for the job queue and a lease-timeout/reclaim mechanism for
  crashed-mid-batch `'processing'` jobs were both deliberately not built —
  neither is meaningful at this app's scale (fast, lightweight jobs; a rare
  edge case) and both would have been UI/complexity for their own sake.
