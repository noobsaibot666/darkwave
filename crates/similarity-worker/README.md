# similarity-worker

A standalone binary, **not** a library used by the main app. It shells out to
[bliss-rs](https://github.com/Polochon-street/bliss-rs) to compute an audio
similarity feature vector for one file at a time, printing the result as JSON
on stdout.

## Why a separate crate

bliss-rs is GPL-3.0. The rest of this workspace is `license = "Proprietary"`
(root `Cargo.toml`). Linking bliss-rs directly into `apps/desktop/src-tauri`
would put GPL-3.0 code in the same binary as Proprietary code, which is a real
license conflict, not a formality.

Instead, this crate is built as its own binary with its own explicit
`license = "GPL-3.0-or-later"` (not `license.workspace = true`), and the main
app spawns it as a subprocess (a Tauri sidecar) rather than depending on it as
a library. Only this one small binary — and its own source, which is public
upstream via bliss-rs anyway — carries the GPL-3.0 obligation. The main app
binary never links against GPL-3.0 code.

See `docs/adr/0025-real-audio-analysis.md` for the full reasoning, including
why bliss-rs is used only for the similarity vector and not for a literal BPM
number (its public `Analysis::Tempo` value is normalized for distance
comparison, not a real BPM — see the ADR).

## Usage

```
similarity-worker <path-to-audio-file>
```

Prints one line of JSON to stdout: `{"analysis": [f32; N]}` on success, or
`{"error": "..."}` on failure (with a non-zero exit code).
