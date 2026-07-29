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
- Update system.
- Signing and notarization.

The update system and signing/notarization gates remain planned until distribution credentials and release channels are configured.

Before a release candidate, run:

```sh
npm run check
npm run build
cargo test --workspace
```

Manual verification still has to cover audio playback, external drag targets, NAS disconnect/reconnect behavior, keyboard navigation, reduced motion/transparency settings, installer behavior, and crash recovery.
