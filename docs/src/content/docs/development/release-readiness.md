---
title: Release Readiness
description: Release candidate gates and distribution checklist.
---

Release readiness is tracked through explicit gates:

- macOS platform audit.
- Windows platform audit.
- Accessibility audit.
- Performance profile.
- Crash recovery.
- Onboarding and documentation.
- Codec packaging.
- Codec license review.
- Update system.
- Signing and notarization.

`release-readiness` exposes a `ReleaseReadinessConfig` that combines source-owned gates with optional distribution metadata. The desktop shell uses this aggregate config, so the release blocker list stays consistent with the same gate model used by tests.

Decode coverage for the analysis pipeline (waveform/tempo/pitch/needs-review/similarity — not playback, which the OS-native `<audio>` element already handles regardless) covers WAV, MP3, FLAC, AIFF, and OGG: WAV PCM decoding is native (hand-rolled `parse_wav_pcm`), and MP3, FLAC, AIFF, and OGG decode through Symphonia (`crates/audio-metadata`'s `SymphoniaDecoder`, wired into production via `decode_any_supported_audio` — see `docs/adr/0025-real-audio-analysis.md`). This is real, tested against actual fixture files, not a stub. That closes the **codec packaging** gate (`REQUIRED_PACKAGED_DECODER_EXTENSIONS` is `["mp3", "flac", "aiff", "ogg"]`).

**Codec license review is closed, for real, not by fiat.** Symphonia itself is MPL-2.0 — safe for both distribution channels, no obligation beyond what its public source already satisfies. The one genuinely open question was AAC-specific: Symphonia's `aac` feature is an independent decoder implementation, not a pass-through to the OS's own (separately, already-licensed) AAC codec, and Via LA's AAC patent pool (merged with MPEG LA in 2022) still charges a per-unit royalty for decoder *software* — unlike MP3 (patents expired) or FLAC/Ogg/Vorbis (royalty-free by design). Rather than leave that open pending a paid legal opinion, V1 simply doesn't link an AAC decoder at all: `crates/audio-metadata`'s Cargo.toml no longer enables Symphonia's `aac`/`isomp4` features, so the compiled binary carries no AAC decoder to license in the first place. See `docs/adr/0028-defer-aac-decode-pending-patent-question.md` for the full reasoning and the revisit path (decoding AAC through each platform's own already-licensed system codec — AVFoundation/Media Foundation — instead of an independent decoder). `codec_license_review_gate` now reports `Passed`, wired via a real `license_review_reference` pointing at that ADR in `release_blockers()`. AAC/M4A files remain importable, taggable, and playable (native `<audio>` playback is unaffected) — only Darkwave's own analysis decoder excludes them.

**A second, separate open question for the Mac App Store build specifically: `crates/similarity-worker`.** It's GPL-3.0-or-later (bliss-rs — see the "License boundary" section of `docs/adr/0025-real-audio-analysis.md`), deliberately run as an isolated sidecar subprocess rather than linked into the Proprietary main binary. That isolation is sound for direct distribution, but Apple's Mac App Store distribution terms have a documented history of friction with GPL, and bundling the sidecar inside an App-Store-distributed bundle is a distinct legal question from how the code is architected internally. Two ways to close this, not a formality either:

- Get an actual legal opinion that bundling it under MAS terms is fine, and ship it as-is on both channels.
- Or simply don't include the `similarity-worker` binary when building the MAS distributable (`tauri build` without `bundle.externalBin` set — a build-invocation decision, not a code change). The app already degrades gracefully without it: `run_similarity_worker` (`apps/desktop/src-tauri/src/lib.rs`) returns `None` if the sidecar can't be found, and the frontend's "Find Similar" action already shows a friendly message rather than failing hard. This means SONIC RADAR's similarity search is simply unavailable on the MAS build, direct-sale keeps it — not something that needs a Cargo feature or UI change to support, just a different `tauri build` invocation at Phase E.

The update system gate now validates source-owned channel metadata: an HTTPS update manifest URL and a non-empty release public-key identifier. The desktop shell still reports the update gate as planned until real channel metadata is configured for a release build.

The direct-sale build's plumbing for this is in place: `tauri-plugin-updater` is wired (direct-dist only — the MAS build relies entirely on Apple's own updater), a real Ed25519 signing keypair exists at `secrets/darkwave-updater.key` (gitignored, **not** in version control — back it up somewhere durable, losing it means old installs can never verify a future signed update again), and `tauri.direct.conf.json` points at a manifest endpoint shaped like `https://alan-design.com/darkwave/updates/{{target}}/{{arch}}/{{current_version}}`. That endpoint doesn't exist as a deployed server yet — it's the same licensing-server generalization work needed for licensing itself (web_three, currently CineFlow-only end to end). `release_blockers()` deliberately does not mark this gate `Passed` until that endpoint is actually live, since this panel is what tells a user the release is ready — a URL that would 404 in production shouldn't read as done.

The signing/notarization gate now validates source-owned identity metadata: macOS Developer ID, macOS team ID, and Windows certificate thumbprint. The desktop shell still reports signing/notarization as planned until real certificates and notarization credentials are configured for a release build.

**Decision: Windows ships unsigned for V1, matching exposeu_wrapkit's (CineFlow Suite) precedent.** No EV code-signing certificate purchase planned right now — buyers will see a SmartScreen "unknown publisher" warning on first run, same as CineFlow's Windows build always has. macOS signing (Developer ID + notarization for direct-sale, Apple Distribution + Mac App Store for the App Store build) still goes ahead fully — see `apps/desktop/scripts/`. Because `SigningNotarizationConfig.has_complete_metadata()` requires all three fields (macOS Developer ID, macOS team ID, *and* Windows certificate thumbprint) to be non-empty, `signing_notarization_gate` will correctly stay `Planned` even once macOS signing is fully wired — that's an accurate reflection of a real, deliberate gap, not a bug to chase. Revisit if Windows sales volume ever justifies the EV cert's cost and lead time.

Before a release candidate, run:

```sh
npm run check
npm run build
cargo test --workspace
```

Manual verification still has to cover audio playback, external drag targets, NAS disconnect/reconnect behavior, keyboard navigation, reduced motion/transparency settings, installer behavior, and crash recovery.
