---
title: App Store Connect Metadata
description: Ready-to-paste copy for the Mac App Store listing, and what's still blocking submission.
---

Draft copy for App Store Connect's listing fields, sized to Apple's actual limits. Adjust before submitting — this is a starting point, not a submission checklist substitute (see the bottom of this page for what's still genuinely blocking).

## App name (30 char limit)

```
Darkwave
```

## Subtitle (30 char limit)

```
Sound Library & SFX Manager
```

## Promotional text (170 char limit, editable anytime without a new review)

```
Drop audio in once, find it instantly forever. Real audio-analysis classification, similarity search, and NAS-ready libraries for editors and sound designers.
```

## Description (4000 char limit)

```
Darkwave turns a folder of imported sounds into a searchable, reusable personal library. The core promise is simple: drop audio in once, find it instantly forever.

REAL CLASSIFICATION, NOT FILENAME GUESSING
Every sound gets classified as Soundtrack, Voiceover, Sound Effect, Foley, or Ambience — using actual decoded-audio analysis (vocal presence, tempo, tonal content), not just parsing filenames or folder names. Tempo, pitch, and vocal ratio are detected automatically.

SONIC RADAR SIMILARITY SEARCH
Find sonically related sounds in your library on demand, based on real audio analysis rather than shared tags alone.

BUILT FOR NAS AND EXTERNAL STORAGE
Point a library at any folder — local, external drive, or NAS share — and Darkwave handles reconnect automatically if the share goes offline and comes back. Your files stay exactly where they are; Darkwave catalogs them without moving or duplicating anything you don't ask it to.

PROJECT-AWARE EXPORT
Send a sound straight to a project and it's filed automatically by type — Music, Voiceover, SFX, or Foley (further split by tag) — matching the folder structure editors already expect.

LOCAL-FIRST, PRIVATE BY DESIGN
No cloud sync, no telemetry, no account required. Your library — files, tags, notes — never leaves your machine.

Built for video editors, sound designers, and anyone maintaining a working sound effects or music library who's tired of re-searching for the same sound twice.
```

## Keywords (100 char limit, comma-separated)

```
audio library,sound effects,sfx manager,foley,sound design,audio tagging,nas audio,music library
```

## Category

Primary: **Music**. (Productivity is a plausible secondary — Apple allows one secondary category at submission time.)

## Age rating

No objectionable content — expect 4+.

## Support URL / Marketing URL / Privacy Policy URL

Point these at the docs site's published versions of [Support](/user-guide/support/), the docs site root, and [Privacy Policy](/legal/privacy-policy/) respectively — **once the docs site is actually deployed to a public URL**. It currently only builds locally (`npm run build` in `docs/`); there's no live hosting configured yet as far as this repo shows. This is a real submission blocker, not just a nice-to-have: App Store Connect requires a live, reachable Privacy Policy URL at submission time.

## What's-New text (first submission)

```
Initial release.
```

## Still genuinely blocking submission (not just content to fill in)

- **Docs site hosting** — see above. Needs a real domain/deploy target before Support/Privacy Policy URLs can be entered.
- **Bundle ID registration** — `dev.darkwave.app` needs to be registered in the Apple Developer portal before an App Store Connect app record can be created for it.
- **Provisioning profile** — needed for `apps/desktop/scripts/mac_sign_and_package_mas.sh` to actually produce a signable `.pkg`; requires the bundle ID above to exist first.
- **Screenshots** — Mac App Store requires screenshots at specific sizes; none exist yet (curated ones, not the raw verification screenshots from development).
- **The codec-license-review and GPL/MAS gates** in [Release Readiness](/development/release-readiness/) — both still open, both are real legal questions, not paperwork.
