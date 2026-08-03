---
title: App Store Connect Metadata
description: Ready-to-paste copy for the Mac App Store listing, and what's still blocking submission.
---

Draft copy for App Store Connect's listing fields, sized to Apple's actual limits. Adjust before submitting — this is a starting point, not a submission checklist substitute (see the bottom of this page for what's still genuinely blocking).

## App name (30 char limit)

Plain "Darkwave" is already taken by another App Store listing (name conflict discovered at submission time, unrelated to trademark — App Store names are globally unique). Using this instead:

```
Darkwave — Sound Library
```

## Subtitle (30 char limit)

"Sound Library" is now in the name itself, so the subtitle leads with a different angle instead of repeating it:

```
SFX & Music Manager
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

Live now at **<https://docs.alan-design.com>**:

- Support URL: `https://docs.alan-design.com/darkwave/user-guide/support/`
- Marketing URL: `https://docs.alan-design.com/darkwave/`
- Privacy Policy URL: `https://docs.alan-design.com/darkwave/legal/privacy-policy/`

Hosted via the same Cloudflare Tunnel + Traefik infrastructure as `alan-design.com` — `docs-nginx` service in `web_three`'s `docker-compose.traefik.yml`, deployed by copying `docs/dist` directly to the NAS (same pattern as the main site) rather than building on the NAS itself. Redeploy after doc changes: rebuild locally (`npm run build` in `docs/`), swap `dist/` into `docs-site/dist` on the shared mount, **then restart the `docs-nginx` container** (`docker restart docs-nginx` on the NAS) — a full directory swap (not an incremental rsync) leaves nginx serving stale/inaccessible file handles otherwise, confirmed the hard way.

## What's-New text (first submission)

```
Initial release.
```

## Submission status

Nothing is blocking submission anymore. For the record:

- **Docs site hosting** — live, as above.
- **Bundle ID registration** — `dev.darkwave.app` registered.
- **Provisioning profile** — in place at `apps/desktop/src-tauri/embedded.provisionprofile`; `mac_sign_and_package_mas.sh` produces a real signed `.pkg`.
- **Screenshots + App Store icon** — real assets at `apps/desktop/macappstore/` (1024×1024 icon, no alpha; screenshots at 2560×1600). Build already uploaded to App Store Connect and reports `VALID_BINARY` / `APP_STORE_ELIGIBLE`.
- **Codec-license-review and GPL/MAS gates** — both closed, see [Release Readiness](/darkwave/development/release-readiness/).

What's left is entirely inside the App Store Connect dashboard (App Privacy questionnaire, age rating, pricing tier, attaching the uploaded build to a version, and submitting) — a UI workflow, not something scriptable from here.
