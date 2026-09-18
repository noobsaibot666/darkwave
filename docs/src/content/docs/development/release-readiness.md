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

**A second, separate open question for the Mac App Store build specifically — now closed: `crates/similarity-worker`.** This used to be a GPL-3.0-or-later sidecar (bliss-rs — see the "License boundary" section of `docs/adr/0025-real-audio-analysis.md`), run as an isolated subprocess rather than linked into the Proprietary main binary. That isolation was sound for direct distribution, but Apple's Mac App Store distribution terms have a documented history of friction with GPL, and bundling the sidecar inside an App-Store-distributed bundle was a distinct legal question from how the code was architected internally — so the MAS build simply shipped without it (`bundle.externalBin: []`), and "Similar Sounds" was unavailable there.

Resolved by removing the GPL dependency instead of routing around Apple's terms: `crates/similarity-worker` is deleted, and `crates/audio-analysis::compute_fingerprint`/`compute_fingerprint_from_mono` compute an equivalent perceptual fingerprint (chroma + spectral centroid/rolloff/flatness/zero-crossing-rate + the already-computed peak/transient/low-frequency/tempo measurements) in-process, as plain DSP with no GPL — or any — third-party dependency beyond what already links into the main binary. See `docs/adr/0033-drop-bliss-rs-similarity-sidecar.md` for the full reasoning and validation. "Similar Sounds" now works identically on every build (MAS included) and every platform (including Windows, which never had a working sidecar for this at all — cross-platform sidecar builds were explicitly future work per ADR 0025's own consequences section). An existing library needs one re-analysis pass (Sonic Radar's "Backfill" action, or a fresh import) before `similar_assets` has anything to rank against — a fingerprint computed by the old bliss-rs sidecar was a different shape and is never compared against a new one (`similar_assets` already skips any stored vector whose length doesn't match).

The update system gate validates source-owned channel metadata: an HTTPS update manifest URL and a non-empty release public-key identifier. It's `Passed` for real now, not just wired: `tauri-plugin-updater` is enabled (direct-dist only — the MAS build relies entirely on Apple's own updater), a real Ed25519 signing keypair exists at `secrets/darkwave-updater.key` (gitignored, **not** in version control — back it up somewhere durable, losing it means old installs can never verify a future signed update again), and `tauri.direct.conf.json` points at `https://alan-design.com/licensing/darkwave/updates/{{target}}/{{arch}}/{{current_version}}` — a genuinely live route on `web_three/licensing-server` (`GET /darkwave/updates/:target/:arch/:currentVersion` and `GET /darkwave/updates/download/:filename`, added alongside that repo's PRODUCTS registry). Verified end to end by curling it directly: a stale-version request returns real signed-release JSON, a current-version request correctly returns `204`, and the download route serves the actual notarized DMG with a `content-length` matching the real file. Per-release version/signature metadata lives in `licensing-server/releases-meta/darkwave.json`, updated once per release rather than baked into server code — bump it (and the signature, via `tauri signer sign`) the next time a new build ships. Not license-key gated: the real security boundary is the Ed25519 signature the client verifies locally against the public key embedded at build time, not URL secrecy.

The signing/notarization gate validates source-owned identity metadata: macOS Developer ID, macOS team ID, and Windows certificate thumbprint. macOS signing is now real and wired: `apps/desktop/scripts/deploy_direct_macos.sh` produces an actually notarized, stapled, Gatekeeper-accepted DMG (`spctl -a -t open` reports `accepted` / `source=Notarized Developer ID`) using a real Developer ID Application certificate pinned by fingerprint, and `mac_sign_and_package_mas.sh` produces a real `3rd Party Mac Developer Application`/`Installer`-signed `.pkg` ready for App Store Connect upload. `release_blockers()` passes both real identities into `SigningNotarizationConfig`.

**Decision: Windows ships unsigned for V1, matching exposeu_wrapkit's (CineFlow Suite) precedent.** No EV code-signing certificate purchase planned right now — buyers will see a SmartScreen "unknown publisher" warning on first run, same as CineFlow's Windows build always has. Because `SigningNotarizationConfig.has_complete_metadata()` requires all three fields (macOS Developer ID, macOS team ID, *and* Windows certificate thumbprint) to be non-empty, `signing_notarization_gate` correctly stays `Planned` even with macOS signing fully wired and real — `windows_certificate_thumbprint` is deliberately left empty. That's an accurate reflection of a real, deliberate gap, not a bug to chase. Revisit if Windows sales volume ever justifies the EV cert's cost and lead time.

Before a release candidate, run:

```sh
npm run check
npm run build
cargo test --workspace
```

Manual verification still has to cover audio playback, external drag targets, NAS disconnect/reconnect behavior, keyboard navigation, reduced motion/transparency settings, installer behavior, and crash recovery.
