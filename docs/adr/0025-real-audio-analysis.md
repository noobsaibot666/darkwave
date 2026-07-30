# ADR 0025: Real audio analysis — decode, needs-review, auto-tagging, tempo, pitch, similarity

## Status

Accepted.

## Context

ADR 0024 evaluated audioFlux for real audio measurement and deferred it: no PCM
decode pipeline existed yet, and audioFlux itself is a C library needing FFI
bindings. This pass builds that decode pipeline and, with it, four concrete
features requested directly: real (not just file-size) needs-review
detection, real (not filename-guessed) action-tag suggestions, tempo/pitch
metadata, and a "find similar sounds" feature — using a pure-Rust stack
(Symphonia, a hand-written DSP pass, and bliss-rs) chosen after evaluating and
rejecting librosa and pyAudioAnalysis (both pure Python — would require
bundling a Python runtime as a Tauri sidecar, a heavier and more fragile
packaging story than linking a C-compatible Rust crate).

Two pieces of existing scaffolding made this materially smaller than a
from-scratch build: `crates/storage`'s `assets` table already had
`duration_ms`, `sample_rate`, `bit_depth`, `channels`, `loudness_lufs`,
`peak_db`, `bpm`, `bpm_confidence`, `musical_key`, `key_confidence`,
`perceptual_fingerprint` columns that no Rust code read or wrote — clearly
reserved for exactly this. And `crates/audio-metadata`'s
`PackagedAudioDecoder` trait had existed since the MVP decode work with zero
production implementations — a `FixturePackagedDecoder` in tests was the only
thing that ever satisfied it. This ADR fills both in rather than building new
parallel structures.

## Decision

**License boundary.** The workspace's root `Cargo.toml` declares
`license = "Proprietary"` for all crates. bliss-rs (used for the similarity
feature) is GPL-3.0. Rather than link it into the main app binary — which
would put GPL-3.0 code in the same binary as Proprietary code, a real
conflict, not a formality — bliss-rs lives entirely inside a new standalone
binary crate, `crates/similarity-worker`, with its own explicit
`license = "GPL-3.0-or-later"` (deliberately not `license.workspace = true`).
The main app spawns it as a Tauri sidecar subprocess and reads one line of
JSON from its stdout. Everything else in this pass (Symphonia, the
hand-written DSP, `pitch-detection`) is MIT/Apache-2.0/MPL-2.0 and links
directly into the main binary with no conflict.

**Decode: Symphonia plugged into the existing seam.** `SymphoniaDecoder`
(`crates/audio-metadata/src/symphonia_decoder.rs`) implements
`PackagedAudioDecoder` — the trait `decode_supported_audio` has always
dispatched to for mp3/flac/aac/m4a/ogg/aiff, previously satisfied by nothing
in production. WAV keeps using the existing hand-rolled `parse_wav_pcm`
(it's simple enough to not need a general-purpose decoder, and rewriting a
working, tested parser for no reason isn't the goal here). A new
`decode_any_supported_audio` convenience function wires the two together for
callers that don't want to construct their own `PackagedAudioDecoder`.
Verified with a real hand-built AIFF fixture (including a from-scratch
80-bit IEEE-754 extended-precision float encoder for the sample-rate field —
AIFF's one genuinely awkward corner) decoded through the actual Symphonia
code path, not a mock.

**Real needs-review: additive, not a replacement.** The existing size-based
check (`≤ 8KB` at import time, `crates/import-pipeline`) stays exactly as it
is — it's a free, synchronous, zero-decode first pass that catches an entire
class of problem (truncated downloads, sync stubs) before any audio ever
gets touched. `audio_analysis::is_likely_silent_or_corrupt` runs afterward,
as a background job, once real samples exist: RMS across the whole decoded
buffer below roughly -60dBFS (or an empty/undecoded buffer) flags
`needs_review`, the same media_type value the size check already produces —
one UI filter, two independent ways to end up flagged. Crucially this can
catch what size alone never could: a corrupt-but-large file.

**Real action-tag suggestions: rule-based, not ML, and honest about it.**
`crates/audio-analysis` was previously a stub — `AudioMeasurements` and
`intensity_score` existed with fabricated test data and zero real callers.
It now computes real `peak_db`, `transient_density` (onset counting via a
20ms energy envelope, no FFT needed), and `low_frequency_energy` (a simple
one-pole low-pass filter's RMS ratio) from actual decoded samples, then
applies threshold rules: short duration + high transient density + loud →
**Impact**; broadband energy, no sharp transient, moderate duration →
**Whoosh**; energy trending upward across the clip → **Rise**. These are the
first thing that can ever suggest "Rise" — the filename/metadata suggestion
vocabulary (`crates/search`) never matched it. Suggestions are applied via
the existing `suggest_tag_for_asset` with `TagOrigin::AcousticModel`, an enum
variant that has existed since the tag system was built and had zero real
callers until now.

**Tempo: hand-written, not bliss-rs.** bliss-rs's public `Song.analysis`
vector does have a `Tempo` slot, but it's a *normalized* value (observed:
`0.3846389` for a real track), meant only for its own Euclidean-distance
similarity math — not a literal BPM. The only place bliss-rs computes an
actual un-normalized tempo internally is gated behind
`#[cfg(feature = "bench")]` and marked `#[doc(hidden)]`: explicitly not
public API, and not something to depend on. So a literal, human-readable BPM
comes from a small hand-written time-domain autocorrelation estimator in
`crates/audio-analysis` instead: RMS energy envelope, autocorrelate across
lags corresponding to 40–220 BPM, peak-pick, derive a confidence from how
sharp that peak is relative to the mean. No new dependency. Verified against
a synthetic click train at a known BPM. Best-effort, explicitly labeled as
such in the UI — not studio-grade beat detection.

**Pitch: `pitch-detection` (MIT), labeled honestly.** McLeod pitch tracking
over a representative window from the middle of the clip gives a frequency
and a clarity/confidence value, converted to the nearest note name. This is
a monophonic estimate — it will not reliably identify anything about dense
polyphonic music. The UI calls it "detected pitch," never "musical key,"
matching the same honesty standard applied to tempo.

**Similarity: bliss-rs, used for what it's actually good at.**
`crates/similarity-worker` calls `bliss_audio::decoder::symphonia::SymphoniaDecoder::song_from_path`
(bliss-rs's own Symphonia-backed decoder — self-contained, doesn't share the
main app's decoder) and prints `song.analysis.as_vec()` as JSON. The main app
stores that vector (JSON text) in the already-existing `perceptual_fingerprint`
column and computes plain Euclidean distance in-process for "Find Similar
Sounds" — brute-force over all analyzed assets in the library, which is fine
at desktop-library scale (hundreds to low thousands of assets), no vector
index needed.

**Job pipeline: a new kind, same job-queue architecture, mutex lesson
re-applied.** `JobKind::AudioAnalysis` follows the existing three variants
exactly, enqueued at import time alongside `MetadataExtraction`/
`WaveformGeneration`. Its processor, `process_audio_analysis_jobs`, is
modeled on `process_pending_jobs` but does NOT copy its mutex-holding
pattern: this job's per-asset work (decode, DSP, spawning and awaiting a
subprocess) is real time, the exact category of operation ADR 0023/0024
already identified as unsafe to do while holding `CatalogState`'s mutex. The
catalog is locked only for brief synchronous reads (job list, asset lookup,
path resolution) and brief synchronous writes (`set_audio_analysis`,
`complete_job`/`fail_job`) — never across decode, DSP, or the subprocess
`.await`. Referenced/NAS assets not yet warmed into the local preview cache
are left pending rather than failed, so a later cache-warm-and-retry picks
them up, mirroring how playback already treats an uncached path.

## Consequences

- `crates/similarity-worker`'s dev-mode sidecar binary must be built and
  placed at `apps/desktop/src-tauri/binaries/similarity-worker-<target-triple>`
  before `tauri dev`/`tauri build` will find it — `scripts/build-similarity-worker-sidecar.sh`
  automates this for the current machine's triple. `bundle.active` is still
  `false` in `tauri.conf.json` (no installer pipeline running yet), so
  cross-platform sidecar builds for a real release remain future work, not
  something this pass needed to solve.
- Two different Symphonia versions exist in the dependency graph (0.5.5 for
  `audio-metadata`'s direct use, 0.6.0 pulled transitively by bliss-rs in the
  isolated `similarity-worker` binary) — not a conflict, since they compile
  into entirely separate binaries, but worth knowing about if either gets
  bumped independently. `similarity-worker`'s `rust-version` is pinned to
  1.85 explicitly (not the workspace's 1.80) to match what its transitive
  symphonia 0.6 dependency actually requires.
- `duration_ms`/`sample_rate`/`channels`/`bpm`/`bpm_confidence`/`musical_key`/
  `key_confidence`/`peak_db` are now real, populated fields. `loudness_lufs`
  and `bit_depth` remain unpopulated — not computed in this pass (LUFS needs
  a proper loudness-metering algorithm, and post-decode f32 samples don't
  carry original bit depth) — the columns stay reserved for later, same as
  the whole set was before this ADR.
- Rule-based Impact/Whoosh/Rise tagging and the BPM estimator are
  deliberately simple heuristics, not trained models. They're tunable via
  named constants in `crates/audio-analysis` if real-world results call for
  it, but this pass didn't attempt a corpus-validated accuracy pass.
