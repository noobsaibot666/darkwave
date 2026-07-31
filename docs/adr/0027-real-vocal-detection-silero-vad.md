# ADR 0027: Real vocal detection via Silero VAD

## Status

Accepted.

## Context

The player's mood-based accent coloring (`classifyPlayerMood` in `App.tsx`)
needed to distinguish "soundtrack" from "soundtrack with vocals" and "voice
over," and previously could only infer vocal presence from tags — a guess,
not a measurement. The user asked for a real, free, open-source detector
that could plug into the existing `audio-analysis` background-job pipeline
(ADR 0025) with minimal manual upkeep.

## Decision

**Silero VAD via the `voice_activity_detector` crate (MIT).** This is the
app's first native ML inference dependency. The crate wraps the Silero VAD
ONNX model and depends transitively on `ort` (`ort`/`ort-sys`, Apache-2.0),
the Rust ONNX Runtime binding.

**Resampling: hand-rolled, not a new dependency.** Silero only accepts 8kHz
or 16kHz mono input. `crates/audio-analysis::resample_linear` does a plain
linear-interpolation resample of a downmixed copy of the decoded buffer
(never touching the original) rather than pulling in `rubato` or similar —
consistent with ADR 0025's preference for small hand-written DSP over new
dependencies when the accuracy bar is "representative enough for a
classifier," not "broadcast quality."

**Graceful degradation, not a hard dependency.** `detect_vocal_ratio`
returns `Option<f32>` at every failure point: too-short clips, a
model-load failure (`.build().ok()?`), or zero usable chunks all resolve to
`None` rather than propagating an error or panicking. `asset_vocal_ratio`
(the Tauri command) and the frontend (`App.tsx`) both already treat
"no vocal ratio yet" as a normal, expected state (analysis pending, or the
model genuinely unavailable) — the player mood simply falls back to its
tag-only classification. A broken or missing ONNX runtime at the OS level
degrades a color accent; it does not crash playback or the app.

**Scope: player-mood coloring only, for now.** The computed `vocal_ratio` is
stored per-asset (`assets.vocal_ratio`, migrated via `ensure_column`) and
surfaced through `asset_vocal_ratio`, but it is not yet exposed as a search
filter, an inspector-visible value, or a Section 7.3 "Vocal / Instrumental"
tag facet. That remains a natural follow-up (see the V1 readiness review,
`docs/src/content/docs/product/roadmap.md`), not a gap in this decision.

## Consequences

- **Build-time network dependency.** `ort-sys`'s build script downloads a
  prebuilt ONNX Runtime binary for the host platform the first time
  `cargo build`/`cargo test` touches `audio-analysis` (it depends on
  `ureq`, `tar`, `flate2` for exactly this). A machine or CI runner without
  outbound network access at build time will fail to build the whole
  workspace, not just this crate. This has not yet been exercised on
  Windows — `ci.yml`'s `windows-latest` job runs `cargo test --workspace`
  but this dependency was added after the last successful push, so the
  first real signal will come from the next CI run.
- **No packaging story yet for the ONNX Runtime shared library.**
  `tauri.conf.json` has `bundle.active: false` and no `bundle.resources`
  entry for the onnxruntime `.dll`/`.dylib`/`.so`. This is not a regression
  — no installer pipeline exists yet for anything (Milestone 7 is
  deliberately "Not started," per the genesis plan) — but it means the
  moment bundling is turned on, this crate needs the same treatment
  `externalBin`/`similarity-worker` already gets (ADR 0025's Consequences):
  either bundle the runtime library as a resource and point `ORT_DYLIB_PATH`
  at it, or switch to a statically-linked `ort` feature. Tracked as a
  Milestone 7 prerequisite, not fixed here.
- **Async request/response ordering.** The frontend fetches `vocal_ratio`
  per selected asset (`asset_vocal_ratio`) in the same `useEffect` that
  reloads tags on selection change. A monotonic request-id guard
  (mirroring the existing `peakRequestId` pattern used for waveform peaks)
  was added so a slow response for a previously-selected asset can never
  overwrite the vocal ratio — and therefore the player accent color — of
  whatever is selected by the time it resolves.
