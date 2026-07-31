---
title: V2 Ideas
description: A running list of deferred feature ideas — not a roadmap, not committed work.
---

Ideas that come up while working on V1 but don't belong in it — either too large, too speculative, or dependent on something V1 doesn't have yet. Each entry is meant to be short: enough for a future pass to pick the idea back up intelligently, not a full spec. Nothing here is scheduled or promised.

## Dominant instrument detection

**What it would do:** Tag each track with its dominant instrument (guitar, piano, drums, etc.), the same way BPM and musical key are detected today.

**Why it's not in V1:** Unlike vocal detection, this isn't a turnkey addition. Silero VAD (already shipped) is a narrow, single-purpose "speech or not" model with a mature Rust wrapper crate (`voice_activity_detector`) that made it nearly free to add. Instrument classification is a broader multi-class problem with no equivalent turnkey crate — it needs a real model, chosen and integrated deliberately.

**Research done (2026-07-31):**

- **Rust inference path already exists in this codebase.** `ort` (pykeio's maintained ONNX Runtime binding, v2.0.0-rc.10) is already a transitive dependency via `voice_activity_detector`, and is already in `Cargo.lock`. Adding a second model means reusing `ort`, not evaluating or introducing a new ML runtime. (`tract` — a pure-Rust alternative used in production at Sonos for real-time audio — is worth a look if `ort`'s native-binary packaging ever becomes a real blocker, but there's no reason to switch just for this.)
- **This inherits an already-tracked, unresolved gap:** ADR 0027 already flags that the ONNX Runtime shared library has no `bundle.resources` entry in `tauri.conf.json` yet — a Milestone 7 (packaging) prerequisite for Silero VAD that instrument detection would depend on too, not a new problem this feature introduces.
- **Two real model candidates found, with a genuine tradeoff, not yet decided between:**
  - [`onnx-community/Musical-Instrument-Classification-ONNX`](https://huggingface.co/onnx-community/Musical-Instrument-Classification-ONNX) — MIT licensed (free to use commercially, no negotiation needed), already in ONNX format. Fine-tuned wav2vec2-base (~95M params — a noticeably heavier bundle than Silero VAD). 9 classes (acoustic/electric guitar, bass, drum set, flute, hi-hats, keyboard, trumpet, violin). Caveat: trained on only 200 isolated samples per class and explicitly documented as "optimized for single instrument classification" — accuracy on a real mixed music track (guitar + drums + bass + vocals at once, the actual shape of most of this app's music library) is unproven and could be poor.
  - [Essentia's `mtg_jamendo_instrument`](https://essentia.upf.edu/models.html) — 40 instrument classes, trained on real full-length polyphonic Jamendo tracks (a much closer match to actual mixed music), with confidence scores. Licensed CC BY-NC-SA 4.0 by default — **not usable in a commercially-shipped app without requesting MTG's proprietary license** (cost/terms unknown, not yet inquired about). Also TensorFlow-native; would need conversion to ONNX, which is an added integration step and risk.
- **No decision made on which to use** — that's a real product/legal call (accuracy vs. bundle size vs. licensing cost/friction) for whenever this gets prioritized, not something to resolve speculatively now.

Sources: [Musical-Instrument-Classification-ONNX](https://huggingface.co/onnx-community/Musical-Instrument-Classification-ONNX) · [Essentia models](https://essentia.upf.edu/models.html) · [ort](https://ort.pyke.io/) · [tract](https://github.com/sonos/tract)

## Extract audio from video files

**What it would do:** Let a video file (a client's rough cut, a reference clip, a screen recording) be imported like any other source, pulling out just its audio track as a real catalog asset instead of requiring the user to already have a separate audio file.

**Why it's not in V1:** Nobody's asked for it until now, and it touches the import pipeline's assumptions about what a "source" is (currently always an audio file).

**Quick scoping (2026-07-31), not full research — this one leans on infrastructure already in the codebase rather than needing new dependencies:**

- **The decode side is mostly already here.** `SymphoniaDecoder` (`crates/audio-metadata`) already demuxes containers to raw PCM for every other format. Symphonia 0.5.5 — the exact version already pinned — ships an `mkv` container feature (covers Matroska *and* WebM) alongside the `isomp4` feature already enabled (covers MP4/MOV, since they're the same underlying container family). Getting MP4/MOV/MKV/WebM audio-track demuxing would mostly mean turning on one more Cargo feature, not adopting a new dependency like `ffmpeg`. Real AVI wouldn't be covered — Symphonia doesn't have a format crate for it.
- **The encode side already exists too.** `export-pipeline` already renders decoded PCM to 24-bit WAV files (see Exporting docs) — the exact step needed to turn an extracted audio stream into a real, storable asset.
- **The one real gap:** `SymphoniaDecoder::decode_packaged_audio` currently picks the *first track with a non-null codec* — fine when every source is audio-only, wrong for a video file where that could just as easily be the video track. Needs to specifically select an audio-typed track, which Symphonia's track metadata supports but this codebase doesn't check for yet.
- **Open product question, not resolved here:** does importing a video keep a reference to the original video file anywhere (for context / re-extraction later), or does it disappear once the audio's pulled out? Affects storage_mode and the asset's provenance story.
