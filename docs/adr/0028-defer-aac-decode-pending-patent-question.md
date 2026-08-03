# ADR 0028: Defer AAC/M4A decode — codec patent question, not a formality

## Status

Accepted.

## Context

ADR 0025 wired Symphonia into `decode_any_supported_audio` covering mp3,
flac, aac, m4a (via the isomp4 container), ogg, and aiff, closing the
codec-packaging gate in `crates/release-readiness`. What that ADR didn't
resolve — flagged later, while preparing V1 for real distribution (Mac App
Store + direct-sale) — is that "we have a decoder" and "we're clear to
ship that decoder" are two different questions, and Symphonia's own
license (MPL-2.0) only answers the first one.

Researched with current (2026) sourcing rather than assumption:

- **MP3**: the relevant patents have expired; decoding (and encoding) MP3
  carries no royalty obligation anywhere at this point.
- **FLAC and Ogg/Vorbis**: royalty-free by design, no patent pool, never
  have been an open question.
- **AIFF**: an uncompressed PCM container — there is no codec to license.
- **AAC**: still actively licensed. Via LA (which absorbed MPEG LA's AAC
  program in a 2022 merger) runs a patent pool that charges a per-unit
  royalty specifically for products that *decode or encode* AAC in
  hardware or software — distinct from the (royalty-free) right to
  distribute already-AAC-encoded content. Symphonia's `aac` feature is an
  independent decoder implementation, not a pass-through to a platform's
  own already-licensed AAC codec, so linking it in means Darkwave itself
  would be the "product company" on the hook for that royalty. This is
  not resolved by Symphonia's own MPL-2.0 license, which only covers
  copyright in Symphonia's source, not the AAC patent pool's separate
  royalty terms.

Two ways to close this, and only one was actually available now:

1. Pay for an AAC license, or
2. Don't ship an AAC decoder in V1.

Option 2 is free, immediate, and doesn't foreclose option 1 later.

## Decision

**AAC and M4A decode are removed from V1**, not just left "open":

- `crates/audio-metadata`'s Cargo.toml no longer enables Symphonia's `aac`
  or `isomp4` features at all — the compiled binary doesn't link an AAC
  decoder, not merely "the app chooses not to call one." This is the
  actual closure of the patent question for V1, not a workaround pending
  legal review.
- `supported_mvp_format`/`codec_support_for_extension` no longer list
  `aac`/`m4a` as decoder-eligible; both now report
  `CodecSupportStatus::Unsupported`, the same "needs conversion" UX path
  any other unrecognized format already gets.
- `crates/import-pipeline`'s `RECOGNIZED_AUDIO_EXTENSIONS` is untouched —
  it was already deliberately broader than the decode-support set (see its
  own doc comment), so `.m4a`/`.aac` files are still importable,
  catalogable, and taggable; they just can't be waveform-analyzed,
  auto-classified by real audio content, or played back through Darkwave's
  own decode path yet. (Playback specifically is unaffected either way —
  the `<audio>` element uses the OS's native codec support regardless of
  what Symphonia can decode; this ADR only concerns the *analysis*
  pipeline's own decoder.)
- `crates/release-readiness`'s `REQUIRED_PACKAGED_DECODER_EXTENSIONS`
  shrinks from 6 to 4: `["mp3", "flac", "aiff", "ogg"]`. With AAC out of
  the required set, every remaining format has a genuinely clear license
  story — which is what actually lets `codec_license_review_gate` pass
  for real (see `apps/desktop/src-tauri/src/lib.rs`'s `release_blockers()`),
  not a fudge.

## Revisit path

AAC support can come back cleanly by decoding through each platform's own
*already-licensed* system codec instead of shipping an independent
decoder — AVFoundation on macOS, Media Foundation on Windows — which
sidesteps the patent question entirely rather than requiring Darkwave to
carry its own AAC license. That's real, separate platform-specific
engineering work, not a config flip, and not undertaken here.
