# ADR 0029: Local-file performance — persisted waveform cache, streamed hashing, parallel import prep, WAL

## Status

Accepted.

## Context

Server-hosted libraries (TrueNAS / LAN mounts) and small local files both
felt fine in practice. A review of the local-file path — specifically
libraries that live on an external HDD / SSD / NVMe drive — turned up one
real repeated cost plus two smaller ones:

1. **Waveform peaks were recomputed from the whole file, in the WebView, on
   every play, with no cache.** `computePeaks` did
   `fetch(convertFileSrc(path))` → `arrayBuffer()` → `decodeAudioData()` on
   the entire source file every time `loadAssetForPlayback` ran — which is
   a single row click. The result lived only in transient React state
   (`setPeaks(null)` on the next load), so re-selecting the same asset
   redid the full read and full decode. On internal NVMe with a 4 MB MP3
   this is imperceptible; on a USB/Thunderbolt drive with a 200 MB WAV/AIFF
   stem it's a full-bus read plus a full WebKit PCM decode per click. The
   scaffolding for the right design was already present but disconnected:
   `waveform::WaveformCache::from_samples` (dead code), a
   `WaveformGeneration` job kind enqueued on import but processed by
   nothing (only the frontend's `mark_waveform_ready` closed it, *after*
   its own client-side decode), and a `waveform_version` column with no
   companion peak storage.

2. **`lightweight_content_hash` slurped the whole file into one `Vec`**
   (`fs::read`) before SHA-256 — a multi-GB stem spiked memory by its full
   length just to be fingerprinted. The name was also a misnomer; the
   comment at the call site admitted it's a full-file digest.

3. **Import hashed files strictly one at a time**, and the catalog opened
   with only `foreign_keys=ON` — no WAL — so background import/analysis
   writes blocked the interactive reads queued behind them on the single
   mutexed connection.

## Decision

### Waveform peaks: generate in Rust once, persist, serve

**Storage.** New `waveform_peaks` table (`asset_id` PK,
`waveform_version`, `sample_rate`, `payload` TEXT, `generated_at`).
`set_waveform_cache` upserts the row and bumps `assets.waveform_version` so
the two stay in lockstep; `get_waveform_cache` reads it; `clear_waveform_cache`
drops it. `relink_asset` calls `clear_waveform_cache` — the relinked file
is a different file, its old shape no longer describes it.

**`payload` is an opaque JSON string owned by the desktop shell**, not a
storage type: `{ peaks, sample_rate, cache }` where `peaks` is the flat
200-bucket transport strip the UI renders (`waveform::WaveformCache::
transport_strip()`) and `cache` is the full multi-resolution row/inspector/
transport min-max payload from ADR 0004, kept for future richer renderers.
`waveform` gained `serde` derives and the `peak_magnitudes` /
`transport_strip` helpers; `storage` stays decoupled from it.

**Generation.** `process_waveform_jobs` is a new Tauri command modelled on
`process_audio_analysis_jobs` — claim a bounded batch, self-heal stuck
claims via `reset_stuck_processing_jobs`, bounded concurrency (4), catalog
mutex held only for the short reads/writes around a `spawn_blocking` decode
(`decode_any_supported_audio`, the same Symphonia seam analysis uses). An
asset not yet warmed into the local cache is left pending, exactly as
analysis does, not failed.

**The common path decodes once.** During import the analysis job already
decodes every file; it now also calls `build_waveform_payload` on that same
buffer and persists it, so `process_waveform_jobs` normally finds nothing
to do. The standalone job is the path for relinks, an analysis-paused
queue, and libraries catalogued before waveform caching existed.

**Frontend.** `loadAssetForPlayback` now: (1) checks a bounded session
`Map` memo; (2) calls `get_waveform` — a hit means the file is never read;
(3) on a miss, runs the `computePeaks` fallback once, renders it, and calls
the new `store_waveform_peaks` so the miss never recurs. `store_waveform_peaks`
clamps and length-caps the client array before persisting. `mark_waveform_ready`
is removed. `waveform_generation` is a first-class entry in `JOB_KINDS`, so
the existing `runJobDrain` / `background-tick` / `job_status` / retry
machinery drives it with no bespoke wiring; `parse_job_kind_field` and
`job_status` learned the kind.

### Streamed content hash

`stream_content_hash` reads the file through a 64 KiB `BufReader` and feeds
`Sha256::update` in chunks. The digest is byte-for-byte identical to the
old one — drop-in with existing `content_hash` values — but memory is
bounded to the buffer regardless of file size.

### Import: lock-free parallel prepare, then sequential commit

`import_file` is split into `prepare_import(path, mode) -> PreparedImport`
(extension check, streamed hash, filename/metadata tag suggestions — all
filesystem work, no catalog) and `commit_prepared_import(catalog, …)`
(register + enqueue jobs + tag seeding + managed copy — the only stage that
needs the lock). `import_file` / `import_file_with_source_context` are now
thin `prepare`+`commit` wrappers, so existing callers and the standing
worker are unchanged.

The shell's `run_batch_import` helper (used by `import_folder`,
`refresh_library`, `scan_import_folder`) runs `prepare_import` across the
file list with `buffer_unordered(4)` on `spawn_blocking`, sorts the results
back into stable path order, then commits them one at a time under the
per-file lock — ADR 0024's "lock per file, not per scan" is preserved, and
a first import of an SSD/NVMe library is no longer serialized on hashing
one file per core. Concurrency of 4 matches `AUDIO_ANALYSIS_CONCURRENCY`:
enough to use several cores locally, not so much that it floods one NAS
mount. `refresh_library` filters already-known paths *before* the prepare
stage so a rescan never re-hashes what it already has.

### SQLite: WAL

`Catalog::open` now also sets `journal_mode=WAL`, `synchronous=NORMAL`,
`busy_timeout=5000`, `temp_store=MEMORY`. The live catalog always lives on
local app-data storage (see `run()`), so WAL's shared-memory requirement is
never in question; the pragma calls are best-effort (`let _ =`) so an
exotic-filesystem catalog handed in elsewhere still opens. WAL lets
in-flight background writes stop blocking the interactive reads behind them
on the single mutexed connection.

## Consequences

- Every local file is now decoded in full at most once ever — during its
  import-time analysis job — and never again for browsing. Playback itself
  was already fine (Tauri's `protocol-asset` does range requests) and is
  untouched; the NAS resilience path (security-scoped bookmarks, offline
  detection, preview cache, `choose_playback_source`) is untouched.
- `WaveformGeneration` used to "complete when the frontend previews a
  sound" (ADR-less, documented only in a `job_status` comment). It is now a
  real backend queue that reports progress like the other kinds. The
  `maintenance_report` "stale waveform cache" finding, which counted
  pending jobs, now normally reads zero.
- The stored `payload` is a shell-owned JSON contract, not a versioned
  storage schema. `waveform_version` bumps on every regenerate; a corrupt
  or unreadable payload is treated as a cache miss (frontend recomputes),
  never an error.
- `prepare_import` / `commit_prepared_import` / `PreparedImport` are now
  public `import-pipeline` API. `import_file` stays as the one-shot form.
- Concurrency for both import-prep and standalone waveform generation is a
  fixed 4, not adaptive to whether the root is local or a network mount —
  macOS can't tell an external SSD from a NAS by path alone, and 4 is a
  safe compromise both ways (same reasoning as ADR 0026's analysis worker).
- BLAKE3 over SHA-256 (faster, effectively I/O-bound) was considered and
  left alone: it's a stored-format change needing migration, and hashing is
  no longer the bottleneck once it's streamed and parallelized.
