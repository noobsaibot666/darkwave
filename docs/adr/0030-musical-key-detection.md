# ADR 0030: Musical key detection (Krumhansl-Schmuckler)

## Status

Accepted.

## Context

Sonic Radar surfaced the app's own analysis results as one-click sidebar
filters: Has Vocals, Instrumental Only, Detected Tempo, Detected Pitch. The
"Detected Pitch" value (`musical_key` column, `estimate_pitch`) is a
*monophonic* McLeod estimate — a single note like "A4" — which ADR 0025
already flagged as "not a musical key" and unreliable on polyphonic music.
There was no real musical-key ("A minor", "C major") detection, and it's
one of the most useful facets for a music/soundtrack library.

## Decision

**Hand-rolled Krumhansl-Schmuckler in `audio-analysis`, no new heavy or C
dependency.** No maintained pure-Rust key/chroma crate exists, and the
usual references (libKeyFinder etc.) are GPL C++, which this build avoids
(ADR 0024). KS is ~150 lines and matches how `estimate_tempo` /
`estimate_pitch` are already done:

- `estimate_key(&DecodedAudioBuffer) -> Option<KeyEstimate>`: mono-downmix,
  bounded centre window (≤ ~65 s), 8192-pt FFT via `rustfft` (already in
  the dependency tree, MIT/Apache), Hann-windowed, hop 4096.
- Fold the 55 Hz–2093 Hz band into a 12-bin pitch-class chromagram.
- Pearson-correlate the chromagram against all 24 rotations of the
  canonical Krumhansl-Kessler major/minor profiles; the best fit wins.
- Below `KEY_MIN_STRENGTH` (0.6 correlation) return `None` — drum loops,
  atonal SFX and noise beds don't get a spurious key.
- Returns `{ key: "C major" | "A minor", tonic, is_minor, strength }`.

**Stored alongside pitch, not replacing it.** New `assets.detected_key`
(TEXT) + `assets.key_strength` (REAL) columns and matching
`AudioAnalysisUpdate` / `AssetRecord` fields; the `AudioAnalysis` job fills
them from the buffer it already decodes. `musical_key` (monophonic pitch)
stays — it's genuinely useful for single-source SFX/ambience/drones, which
the sidebar tooltip now says explicitly.

**Sonic Radar: one binary toggle.** "Detected Key" (`has_key` filter,
`detected_key != null`) sits between Detected Tempo and Detected Pitch,
matching the other toggles. Filtering to a *specific* key was deliberately
left out of this pass — it needs a key picker and isn't the shape of the
rest of the section. The inspector's "Detected Audio Attributes" shows a
"Key" pill next to the existing "Pitch" pill.

## Consequences

- `rustfft` becomes a direct dependency of `audio-analysis` (it was already
  transitive via `pitch-detection`), so nothing new lands in the build.
- Key detection runs only inside the `AudioAnalysis` job. Assets analysed
  before this ships keep `detected_key = NULL` until re-analysed — there's
  no automatic backfill (no equivalent of the pending `WaveformGeneration`
  jobs ADR 0029 could drain). A "re-run analysis" action or a one-time
  re-enqueue is a possible follow-up; new imports get keys immediately.
- KS is a whole-clip tonal estimate: strong on tonal music, `None` on
  atonal/percussive material by design. It does not track key changes
  within a track (reports the dominant key) and has the classic
  relative-major/minor ambiguity KS is known for — acceptable for a
  library filter, not a transcription tool. `key_strength` is stored so a
  confidence cut or display could be layered on later.
- Filter-by-specific-key and any Camelot/harmonic-mixing view remain
  future work.
