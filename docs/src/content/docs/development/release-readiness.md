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

WAV PCM decoding is native. Compressed MVP formats such as MP3, FLAC, AAC/M4A, AIFF, and OGG are tracked as packaged-decoder work until the decoder bundle and codec licensing review are complete. Unsupported formats remain visible with a conversion option.

The update system gate now validates source-owned channel metadata: an HTTPS update manifest URL and a non-empty release public-key identifier. The desktop shell still reports the update gate as planned until real channel metadata is configured for a release build.

Signing/notarization remains planned until distribution credentials and platform release channels are configured.

Before a release candidate, run:

```sh
npm run check
npm run build
cargo test --workspace
```

Manual verification still has to cover audio playback, external drag targets, NAS disconnect/reconnect behavior, keyboard navigation, reduced motion/transparency settings, installer behavior, and crash recovery.
