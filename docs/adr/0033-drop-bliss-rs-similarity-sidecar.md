# ADR 0033: Drop the bliss-rs GPL sidecar — compute "Similar Sounds" in-process

## Status

Accepted.

## Context

ADR 0025 built "Find Similar Sounds" on `bliss-rs`, a GPL-3.0-or-later crate.
Because the workspace is `license = "Proprietary"`, bliss-rs was isolated
into its own standalone binary (`crates/similarity-worker`), spawned as a
Tauri sidecar subprocess rather than linked into the main app — the main
binary never linked GPL-3.0 code, only shelled out to it.

That isolation solved the *linking* problem, but not a separate one:
Apple's Mac App Store distribution terms have a documented history of
friction with GPL — bundling a GPL-3.0 binary inside an App-Store-distributed
bundle is a distribution-terms question, not a same-binary-linking one, and
sidecar isolation doesn't touch it. Rather than get a legal opinion or ship
it and hope, the MAS build simply excluded the sidecar entirely
(`tauri.mas.conf.json`'s `bundle.externalBin: []`), leaving "Similar Sounds"
unavailable there. That gap is what prompted this pass.

Separately, the sidecar approach had never actually worked on Windows:
cross-platform sidecar builds were explicitly called out as future work in
ADR 0025's own consequences section and never got done, so every Windows
build was silently missing this feature too, sidecar or not.

## Decision

**Remove `crates/similarity-worker` and bliss-rs entirely.** Instead of
routing around Apple's terms (a legal exception, a server-side proxy, an
LGPL swap), close the question at its root: there's no GPL — or any
third-party — dependency left to have an opinion about. `crates/audio-analysis`
now computes an equivalent perceptual fingerprint itself, as plain DSP that
links directly into the main binary on every platform, the same way its
tempo/pitch/key estimators already do.

**The new fingerprint (`compute_fingerprint`/`compute_fingerprint_from_mono`,
`FINGERPRINT_DIMENSIONS = 20`):**

- A 12-bin chroma profile — reuses `estimate_key`'s exact FFT size, hop
  size, frame cap, frequency band, and frequency-to-pitch-class mapping, so
  this doesn't introduce a second, differently-tuned chroma computation to
  keep in sync with the one already shipped and tested. Normalized to sum
  to 1 (a proportion vector) rather than raw magnitude, so a loud and a
  quiet recording of the same content don't read as different just from
  level.
- Spectral centroid (brightness), rolloff (how much of the spectrum sits
  below vs. above a cutoff), flatness (tonal vs. noisy), and zero-crossing
  rate — computed in the same windowed-FFT pass as chroma, each normalized
  to a comparable 0..1-ish range.
- The already-computed `AudioMeasurements` (peak level, transient density,
  low-frequency energy) and tempo estimate, each similarly normalized —
  free, since `analyze_asset_audio` (`apps/desktop/src-tauri/src/lib.rs`)
  already computes every one of these for tempo/tag-suggestion/needs-review
  purposes on the same decoded buffer.

`similar_assets` (the Tauri command that ranks by Euclidean distance)
doesn't change at all — it was already just comparing two `Vec<f32>`s of
matching length, with no dependency on what produced them.

**Not a claim of matching bliss-rs's tuned accuracy.** Like this crate's
tempo/pitch estimators, it's an honest, tunable heuristic, not a trained
model. Validated with synthetic-audio unit tests rather than assumed:

- Two identical tones produce a near-zero distance (`fingerprint_distance <
  0.01`), and a vector's distance to itself is exactly zero.
- Pure tones sharing a musical pitch class (A2/A4/A5 — same pitch class,
  three octaves apart, acoustically quite different in raw brightness)
  cluster measurably closer together than a different pitch class does
  (C4, G#4) — including against G#4, whose raw frequency (415.3Hz) sits
  almost on top of A4's (440Hz), confirming chroma is actually driving the
  result rather than raw frequency proximity.
- A pure tone and white noise of the same amplitude land more than twice as
  far apart as two takes of the same tone — flatness and chroma
  concentration correctly separate tonal from noisy content.
- Every input (including an empty buffer and one too short for the FFT
  window) returns a valid `FINGERPRINT_DIMENSIONS`-length vector rather
  than panicking or returning a differently-shaped one.

## Consequences

- **Works identically on every build, MAS included, and on Windows for the
  first time.** No sidecar, no `externalBin`, no per-platform binary to
  cross-compile and ship — one code path, same as tempo/pitch/key. The
  `shell:allow-execute` capability, `tauri-plugin-shell` dependency and
  plugin registration, and the whole resident-subprocess/mutex/timeout
  machinery in `apps/desktop/src-tauri/src/lib.rs` are gone — nothing else
  in the app used the shell plugin.
- **Fingerprinting is no longer detached from job completion.** The old
  sidecar version ran fingerprinting as a fire-and-forget task *after*
  `analyze_asset_audio` returned, specifically because bliss-audio's own
  feature extraction over a full song was genuinely slow enough to be worth
  not gating job completion on. The new computation reuses buffers/
  measurements already produced for tempo/key/pitch in the same
  `spawn_blocking` closure and is bounded by the same ~65s analysis window
  `estimate_key` already uses — cheap enough to just be part of the one
  synchronous `AudioAnalysisUpdate` write. This removes an entire class of
  complexity (a resident-subprocess mutex, a detached task, a narrow
  single-column follow-up write) along with the GPL dependency.
- **An existing library's stored fingerprints don't carry over.** A
  bliss-rs vector and this crate's vector are different shapes; `similar_assets`
  already guards against comparing mismatched lengths (`vector.len() !=
  target_vector.len()`), so an old fingerprint is simply never matched
  against a new one — never a wrong comparison, just a missing one until
  the asset is re-analyzed. Existing libraries need one re-analysis pass
  (Sonic Radar's "Backfill" action, or a fresh import) before `similar_assets`
  has anything current to rank against.
- `crates/storage::Catalog::set_perceptual_fingerprint` (the narrow,
  single-column write the old detached task needed) is deleted along with
  its dedicated test — `perceptual_fingerprint` is just another field on
  the one `set_audio_analysis` call now, like every other analysis result.
- CI (`.github/workflows/ci.yml`) no longer builds a sidecar before
  `cargo test --workspace` — that step existed solely for
  `crates/similarity-worker` and would otherwise fail outright now that the
  crate is gone. Removed, not fixed, since there's nothing left to build.
