# ADR 0031: Instrument detection (YAMNet) as a Sonic Radar facet

## Status

Accepted.

## Context

Sonic Radar surfaces the app's own analysis as sidebar filters (vocals,
tempo, key, pitch). "Which instruments are in this track" is one of the most
useful ways to browse a music/soundtrack library and had no support. Unlike
tempo (ADR 0026) and key (ADR 0030), instrument recognition can't be
hand-rolled from DSP at usable accuracy — it needs a trained multi-label
classifier.

## Decision

**Model: YAMNet ONNX, run in-process via `ort`.** `ort` (ONNX Runtime,
MIT) is already linked for Silero VAD, and the app already bundles one
model's weights, so this is not a new category. YAMNet's weights are
Apache-2.0 and its AudioSet class map CC-BY-4.0 — no GPL, so this runs
in-process rather than in a GPL-isolated sidecar like the similarity
worker. The **waveform-input** YAMNet variant is used: its graph contains
the log-mel frontend, so `crates/audio-analysis/src/instruments.rs` only
resamples to 16 kHz mono, runs inference, mean-pools the per-frame scores,
and folds the instrument-relevant AudioSet classes onto a small clean label
set (`INSTRUMENT_LABEL_RULES` — e.g. "Electric guitar" + "Slide guitar" →
one label; "Speech" → dropped). A class must clear a mean-score threshold
to count; at most 8 instruments per asset.

**The model is not vendored.** `load_instrument_model` returns `None` when
`models/yamnet.onnx` + `yamnet_class_map.csv` are absent (checked in
`<app-data>/models/` first, then the bundled resources dir);
`process_instrument_jobs` then claims nothing and the `InstrumentDetection`
jobs stay pending. Same graceful-degradation contract as the similarity
worker. `tauri.conf.json` bundles `models/*`; the files are gitignored and
documented in `docs/development/instrument-model.md`.

**New `InstrumentDetection` job kind**, enqueued at import beside the
others, drained by `process_instrument_jobs`. It's its own job (a separate
decode) rather than piggybacking on `AudioAnalysis` because the ONNX
`Session` is `Send` but not `Sync`: inference is serialised on the model
mutex (one job at a time — ONNX parallelises internally anyway), which
doesn't fit the `buffer_unordered` shape of the analysis/waveform workers.
The model loads once, lazily, on the first `process_instrument_jobs` call.
Wired into `JOB_KINDS`, `job_status`, `parse_job_kind_field`, and the retry
path like every other kind.

**Storage:** `asset_instruments(asset_id, instrument, confidence)` table.
`instrument_counts_for_library` (the page's facet), `assets_with_any_instrument`
(OR-match filter), `instruments_for_asset` (inspector). A relink clears the
rows and re-enqueues detection.

**UI: a dedicated full-area page.** "Instrument Detection" in Sonic Radar
opens a pill grid of every detected instrument with its track count. Plain
click → that instrument's tracks. Shift-click → toggle into a running
multi-selection; "Show tracks" applies it (OR). In results mode a slim bar
shows the active instrument chips (removable) and a back-to-grid control.
The filter is one `ActiveFilter` shape, `{ instrumentPage, instruments }`,
with `refreshAssets` calling `assets_by_instruments` in results mode. The
inspector's "Detected Audio Attributes" lists the asset's instruments as
click-to-browse chips.

## Consequences

- Ships with **no** model by default: the feature is inert until the file
  is dropped in (documented). This is deliberate — a 15 MB binary blob with
  its own provenance/attribution shouldn't land silently, and CI/tests
  don't need it (the graceful-absence path is what's tested).
- Assets analysed before this ships have no instruments until re-detected;
  as with ADR 0030 there's no automatic backfill for already-`completed`
  work, only for the pending jobs every asset already carries. New imports
  get instruments once a model is installed.
- The label taxonomy is a curated fold of AudioSet classes, not a designed
  instrument vocabulary — good enough for a browse facet, coarser than a
  dedicated OpenMIC/IRMAS model would be. `INSTRUMENT_LABEL_RULES` is the
  one place to tune it.
- Multi-select is OR only. AND ("tracks with piano *and* strings") and a
  Camelot-style harmonic view remain future work.
- Instrument inference is single-threaded (model mutex). Fine at
  desktop-library scale; if it becomes a bottleneck the fix is multiple
  sessions, not a lock change.
