# ADR 0032: Sonic Radar re-analyse ("sync") button

## Status

Accepted.

## Context

Every analysis feature added since the job system matured — real audio
analysis (0025), waveform cache (0029), musical key (0030), instrument
detection (0031) — shares the same gap: an asset whose `AudioAnalysis` /
`WaveformGeneration` / `InstrumentDetection` job already ran to `completed`
never picks up a newer detector. The pending-job drain only helps assets
imported after the feature shipped. Each ADR noted "no automatic backfill".

## Decision

A **sync button in the Sonic Radar section header** re-queues all three
analysis passes for a target scope:

- **Selection** — the browser's multi-selection, or the single selected
  sound. `resync_analysis(library_id, asset_ids)`.
- **Whole library** when nothing is selected (behind a confirm dialog,
  since it re-analyses everything).

Storage: `requeue_analysis_for_library` / `requeue_analysis_for_assets`
delete any of the target's jobs of that kind that aren't `'processing'`,
then insert one fresh `'pending'` job each — so a re-sync never duplicates
a queued job and never disturbs work already in flight. The shell command
loops the three kinds (`WaveformGeneration`, `AudioAnalysis`,
`InstrumentDetection`) and returns the total queued; the frontend then
calls the normal `runJobDrain`, so progress, retry and completion
reporting all work unchanged.

`MetadataExtraction` is deliberately not re-queued — embedded-tag reading
isn't a Sonic Radar concern and re-running it could clobber user edits
(same reasoning as import's is-new guard).

## Consequences

- This is the backfill path the earlier ADRs left open — now user-driven
  rather than automatic, which is the right default: re-analysing a large
  library is minutes of CPU and shouldn't happen unprompted.
- Re-sync respects the same graceful-degradation contracts: a queued
  `InstrumentDetection` job with no model installed just waits (and the
  frontend skips draining that kind — see `instrument_detection_available`
  — so it doesn't flash a 0%-progress bar every tick).
- Re-running `AudioAnalysis` also re-runs the import-time media-type
  "first funnel" (`save_audio_analysis_outcome`) for assets still at
  `media_type = "other"` or an unreviewed `"sound_effect"`. That's
  intended — re-sync means "re-derive from the signal" — but it means an
  un-curated library can see a few sounds change category after a re-sync.
  Anything the user has reviewed or manually typed is untouched.
- `requeue_analysis_for_library` runs bulk `INSERT…SELECT` over the whole
  assets table per kind, so `resync_analysis` is `#[tauri::command(async)]`
  to keep it off the main thread on a large library.
- Scope is "the analysis trio". If a future pass adds a fourth analysis
  job kind, add it to the `kinds` array in `resync_analysis`.
