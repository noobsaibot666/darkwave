# Darkwave — Launch & Promotion Package

Copy is paste-ready. Asset rows give exact filename, pixel size, format. `☐` = to do. §7 is the rollup.

| | |
|---|---|
| Product name | `Darkwave` |
| App Store listing name (24/30) | `Darkwave — Sound Library` — plain "Darkwave" is taken (App Store names are globally unique) |
| Bundle ID / category | `dev.darkwave.app` · Music (secondary: Productivity) |
| Version | `0.2.1` (build 8) — App Store 0.2.0 was already approved, so this is the next increment |
| Price | US$49.99 tier (matches storefront €49) · real charge via Stripe `price_1U05EpCsCSs3k4X1fr7x7aER` |
| Platforms | macOS 12+ (App Store + direct) · Windows (direct only, unsigned) |
| Storefront | `alan-design.com/#/store` — `web_three/src/data/storeProducts.json`, id `darkwave` (exists, `badge: "NEW"`) |
| Docs / support | `https://docs.alan-design.com/darkwave/` (Astro/Starlight, repo `docs/`, base path `/darkwave/`) |
| Social | one brand handle `@alandesign`, link `alan-design.com` |

**Brand rules for every asset:** dark-first, calm, dense, keyboard-friendly. Neutral grey palette — **no warm/beige tint anywhere**. The accent orange `#ff5c00` appears **only** in the player: the transport play button, the waveform progress fill, and the selected row (which mirrors the player's mood colour). Never on buttons, chrome, icons, or headings — those are neutral grey (Final Cut / Logic / Pixelmator register). Player mood colours: Soundtrack green · Soundtrack-vocal purple · Voice-over blue · Sound effect gold · default orange. Capture with the real "Orpheus" library (2739 sounds) but **scrub every real file path, volume name, and license key** before export.

---

## Canonical copy — reuse everywhere

```
One-liner:  A local-first sound library for editors and sound designers — drop audio in once, find it instantly forever.
Short:      Find any sound in seconds
Pitch:      Darkwave turns a folder of imported sounds into a searchable, reusable library, classifying every clip by
            real audio analysis — vocal presence, tempo, tonal content — not filenames. It runs entirely on your
            machine against local, external, or NAS-hosted media, with no cloud, no account, and no telemetry.
```

**Drop audio in once; every clip is analysed, classified, and searchable forever — offline.**

**Proof points:** real decoded-audio classification (Soundtrack / Voiceover / SFX / Foley / Ambience), Sonic Radar similarity search, automatic tempo + pitch + vocal detection, NAS & external-drive libraries with auto-reconnect, project-aware export by folder taxonomy, fully local — no cloud / telemetry / account, macOS + Windows.
**Audience:** video editors, sound designers, and anyone maintaining a working SFX or music library.
**Hashtags:** `#sounddesign #videoediting #audiolibrary #sfx #foley #postproduction #filmmaking #davinciresolve`

---

## 1. App icon

A rebaked `.icns` shipped this cycle (from the new Icon Composer source). The existing `apps/desktop/macappstore/icon/darkwave-appstore-icon-1024.png` predates that rebake — **regenerate it**.

| ✓ | File | Size | Format | Notes |
|---|---|---|---|---|
| ☐ | `apps/desktop/macappstore/icon/darkwave-appstore-icon-1024.png` | 1024×1024 | PNG, no alpha, sRGB/P3 | flatten current icon, extend gradient full-bleed, **remove** rounded-corner mask (Apple masks on display) |

- ☐ Pipeline: render the master from the current `.icon` → flatten on the app background → resize to exactly 1024×1024 → strip alpha → verify `sips -g hasAlpha` reports `no`
- ☐ Must match the in-app icon pixel-for-pixel (same rebaked source)

---

## 2. Mac App Store

### 2.1 Text fields

| ✓ | Field | Limit | Value |
|---|---|---|---|
| ☐ | Name | 30 | `Darkwave — Sound Library` — 24/30 |
| ☐ | Subtitle | 30 | `SFX & Music Manager` — 19/30 |
| ☐ | Promotional text | 170 | block below — 158/170 |
| ☐ | Keywords | 100, no space after commas | `sfx,foley,sound design,audio tagging,nas audio,ambience,voiceover,similarity search,sound catalog` — 97/100 |
| ☐ | Description | 4000 | block below — 1530/4000 |
| ☐ | What's New | 4000 | `Initial release.` |
| ☐ | Support URL | — | `https://docs.alan-design.com/darkwave/user-guide/support/` |
| ☐ | Marketing URL | — | `https://docs.alan-design.com/darkwave/` |
| ☐ | Privacy Policy URL | — | `https://docs.alan-design.com/darkwave/legal/privacy-policy/` |
| ☐ | Category / Age | — | Music · Productivity (secondary) · 4+ |
| ☐ | Copyright | — | `© 2026 Alan Alves` (confirm trading name — §8) |

Keywords note: `sound`, `library`, `music`, `sfx`, `manager` already appear in name/subtitle and are indexed from there — kept out of the keyword field on purpose.

**Promotional text** (158/170):
```
Drop audio in once, find it instantly forever. Real audio-analysis classification, similarity search, and NAS-ready libraries for editors and sound designers.
```

**Description** (1530/4000):
```
Darkwave turns a folder of imported sounds into a searchable, reusable personal library. The core promise is simple: drop audio in once, find it instantly forever.

REAL CLASSIFICATION, NOT FILENAME GUESSING
Every sound is classified as Soundtrack, Voiceover, Sound Effect, Foley, or Ambience from actual decoded-audio analysis — vocal presence, tempo, tonal content — not by parsing filenames or folders. Tempo, pitch, and vocal ratio are detected automatically as sounds come in.

SONIC RADAR SIMILARITY SEARCH
Find sonically related sounds in your library on demand, based on real audio analysis rather than shared tags alone. Filter instantly by "has vocals", "instrumental only", detected tempo, or detected pitch.

BUILT FOR NAS AND EXTERNAL STORAGE
Point a library at any folder — local disk, external drive, or NAS share — and Darkwave reconnects automatically if the share drops and comes back. Your files stay exactly where they are; nothing is moved or duplicated unless you ask.

PROJECT-AWARE EXPORT
Send a sound straight to a project and it is filed by type automatically — Music, Voiceover, SFX, or Foley, further split by tag — matching the folder structure editors already expect.

LOCAL-FIRST, PRIVATE BY DESIGN
No cloud sync, no telemetry, no account. Your library — files, tags, source and license notes — never leaves your machine.

Built for video editors, sound designers, and anyone maintaining a working sound effects or music library who is tired of searching for the same sound twice. macOS and Windows.
```

### 2.2 Screenshots

**Spec:** 2560×1600 · 16:10 · PNG, no alpha, sRGB · up to 10 · pad (don't crop) to size on the dark backdrop · clean demo data, no real paths/keys. First 3 are what shows in search.

| ✓ | # | Filename | Screen / state | Caption (≤6 words) |
|---|---|---|---|---|
| ☐ | 01 | `darkwave-appstore-01-library.png` | Main library — sound list, Sonic Radar sidebar, Tags/Apply Tag inspector, transport playing | Your whole sound library, searchable |
| ☐ | 02 | `darkwave-appstore-02-sonic-radar.png` | Sonic Radar filters open + "Find Similar" results on a selected clip | Find sounds that sound alike |
| ☐ | 03 | `darkwave-appstore-03-classify.png` | Quick Actions → Classify panel, a row mid-analysis showing detected tempo/pitch/vocal | Real analysis, not filename guessing |
| ☐ | 04 | `darkwave-appstore-04-export.png` | Editor Workflow panel — sending selection to a project's typed folders | One click into the right folder |
| ☐ | 05 | `darkwave-appstore-05-command-palette.png` | Cmd+K command palette open | Keyboard-first, everything one shortcut away |
| ☐ | 06 | `darkwave-appstore-06-nas.png` | Settings → General, library pointed at a NAS share, status "Online" | Local, external, or NAS — reconnects itself |
| ☐ | 07 | `darkwave-appstore-07-import.png` | First-run "where do you drop new sounds" / watched-folder settings | Drop a folder in, it imports itself |

### 2.3 App Preview video

**Spec:** 1920×1080 · 30 fps · 15–30 s hard limit · `.mov` H.264/ProRes · screen-recording only, no hands/frames · first frame = poster · licensed/owned audio only.

- ☐ `darkwave-appstore-preview-01.mov` — fed from the `library-search` + `sonic-radar` masters (§6)

| Time | On screen |
|---|---|
| 0–3s | Library view, type in search, results filter live |
| 3–9s | Select a clip → transport plays, waveform scrubs; open Sonic Radar → "Find Similar" |
| 9–16s | Similar results appear; drag one to a project → typed folders fill in |
| 16–22s | Cut to Cmd+K palette, run a command; end on the full library, title card |

---

## 3. Storefront — `alan-design.com/#/store`

**How it renders** (no separate mobile files — frame for both):

| Surface | Desktop | Mobile (<960px) |
|---|---|---|
| Card `image` | 16:10, `cover`, ~453×283 CSS (@2x ≈ 906×566) | 1-col, up to ~768×480 CSS (@2x ≈ 1536×960) |
| Modal `gallery` item | ~720 CSS wide, stacked | full-width 100vw×50vh, scroll-snap, crops ~4:3 — keep content centred |

### 3.1 `storeProducts.json` entry

- ☐ Replace the `darkwave` object in `web_three/src/data/storeProducts.json`:

```json
{
  "id": "darkwave",
  "name": "Darkwave",
  "category": "Apps",
  "description": "Local-first sound library for editors — drop audio in once, find it instantly forever.",
  "longDescription": "Darkwave turns a folder of imported sounds into a searchable, reusable personal library. Real decoded-audio analysis — not filenames — classifies every clip as Soundtrack, Voiceover, Sound Effect, Foley, or Ambience, detects tempo, pitch, and vocal presence, and finds sonically similar sounds on demand. Point a library at a local disk, external drive, or NAS share; Darkwave reconnects automatically and never moves your files. Fully offline, no account, no telemetry.",
  "features": [
    "Real audio-analysis classification, not filename guessing",
    "Sonic Radar similarity search finds sonically related sounds",
    "Automatic tempo, pitch, and vocal-presence detection",
    "NAS and external-drive libraries with automatic reconnect",
    "Project export folder taxonomy (Music / Voiceover / SFX / Foley)",
    "Fully local — no cloud sync, no telemetry, no account",
    "macOS and Windows"
  ],
  "price": "€49",
  "stripePriceId": "price_1U05EpCsCSs3k4X1fr7x7aER",
  "downloadUrl": "https://alan-design.com/#/download",
  "image": "/assets/images/store/darkwave/darkwave-hero.webp",
  "gallery": [
    { "type": "video", "url": "/assets/videos/darkwave-loop.mp4" },
    { "type": "image", "url": "/assets/images/store/darkwave/darkwave-01-library.webp" },
    { "type": "image", "url": "/assets/images/store/darkwave/darkwave-02-sonic-radar.webp" },
    { "type": "image", "url": "/assets/images/store/darkwave/darkwave-03-classify.webp" },
    { "type": "image", "url": "/assets/images/store/darkwave/darkwave-04-export.webp" },
    { "type": "image", "url": "/assets/images/store/darkwave/darkwave-05-nas.webp" }
  ],
  "stripeLink": "#",
  "stripeMode": "payment",
  "badge": "NEW",
  "appStoreUrl": ""
}
```

- ☐ Set `appStoreUrl` once the Mac App Store listing is live (adds the card ribbon + modal badge) · ☐ drop `badge` to `""` when no longer new
- **Caveat:** a Store Manager **Publish** rewrites this file. Put final copy in the Store Manager `manifest.json`, or hand-edit `storeProducts.json` only *after* the last publish — see §8.

### 3.2 Assets

| ✓ | Asset | Path | Size | Budget |
|---|---|---|---|---|
| ☐ | Card thumbnail | `web_three/public/assets/images/store/darkwave/darkwave-hero.webp` | 1600×1000 | <250 KB |
| ☐ | Gallery loop | `web_three/public/assets/videos/darkwave-loop.mp4` | 1600×1000, 12–20 s, muted | <8 MB |
| ☐ | Gallery still 01 | `.../store/darkwave/darkwave-01-library.webp` | 1920×1200 | <400 KB |
| ☐ | Gallery still 02 | `.../store/darkwave/darkwave-02-sonic-radar.webp` | 1920×1200 | <400 KB |
| ☐ | Gallery still 03 | `.../store/darkwave/darkwave-03-classify.webp` | 1920×1200 | <400 KB |
| ☐ | Gallery still 04 | `.../store/darkwave/darkwave-04-export.webp` | 1920×1200 | <400 KB |
| ☐ | Gallery still 05 | `.../store/darkwave/darkwave-05-nas.webp` | 1920×1200 | <400 KB |

Stills = the §2.2 captures **without** caption overlays, re-exported to WebP; keep key UI centred (mobile crops ~4:3).

---

## 4. Astro docs site — `docs/` → `docs.alan-design.com/darkwave/`

Stills → `docs/public/screenshots/`, media → `docs/public/media/`. Body image links **must** include the base prefix: `![](/darkwave/screenshots/<file>)`.
**Spec:** capture 2560×1600 → WebP ≤1600 px wide, q≈82, <400 KB.

| ✓ | File | Doc page | Shows |
|---|---|---|---|
| ☐ | `darkwave-docs-getting-started-01-library.webp` | `user-guide/getting-started` | library view, labelled regions |
| ☐ | `darkwave-docs-importing-01-watched.webp` | `user-guide/importing` | watched-folder / import-folder settings |
| ☐ | `darkwave-docs-organizing-01-tags.webp` | `user-guide/organizing` | Tags / Apply Tag inspector, a tag being applied |
| ☐ | `darkwave-docs-searching-01-sonic-radar.webp` | `user-guide/searching` | Sonic Radar filters + Find Similar |
| ☐ | `darkwave-docs-exporting-01-workflow.webp` | `user-guide/exporting` | Editor Workflow project routing |
| ☐ | `darkwave-docs-backup-01-restore.webp` | `user-guide/backup-and-recovery` | Settings → backup / restore panel |
| ☐ | `darkwave-docs-licensing-01-activate.webp` | `user-guide/licensing-and-activation` | Activate License dialog (dummy key) |
| ☐ | `darkwave-docs-troubleshooting-01-activity.webp` | `user-guide/troubleshooting` | Background Activity panel mid-job |

**Other art:**
- ☐ `darkwave-docs-og.png` — 1200×630 (title + one-liner on the dark ground)
- ☐ favicon 512×512 PNG (the rebaked mark)
- ☐ Deploy: rebuild `docs/` → swap `dist/` into the NAS mount → `docker restart docs-nginx` (full swap, not rsync — stale handles otherwise)

---

## 5. Social

### 5.1 Copy bank
```
Hooks:
- You searched for that exact sound last month. Again.
- Your SFX library doesn't know what's in it. Darkwave does.
- Not "kick_final_v3.wav". Actual audio analysis.
- 2739 sounds. Zero folders opened.
CTA:
- Buy once, own it — alan-design.com  (link in bio)
Don't: no hype adjectives, no "2× faster", no naming other apps, no stock-music beds on paid/App Store cuts.
```

### 5.2 Instagram

**Feed carousel** — 1080×1350, PNG, 6 slides, text ≥96 px from edges. `darkwave-ig-carousel-0N-<slug>.png`.

| ✓ | # | Headline | Sub |
|---|---|---|---|
| ☐ | 01 | Find any sound in seconds | A local-first library for editors & sound designers |
| ☐ | 02 | It listens, then classifies | Soundtrack · Voiceover · SFX · Foley · Ambience — from the audio, not the filename |
| ☐ | 03 | Sonic Radar | "Find similar" pulls sounds that actually sound alike |
| ☐ | 04 | Tempo, pitch, vocals — detected | Automatically, as sounds come in |
| ☐ | 05 | Your NAS, handled | Point at any share; it reconnects itself, never moves files |
| ☐ | 06 | Offline. No account. | alan-design.com — buy once |

- ☐ Publish carousel post:
```
Darkwave — find any sound in seconds. Drop audio in once and every clip gets classified by real audio analysis (vocal presence, tempo, tonal content), not filenames. Sonic Radar finds sounds that actually sound alike. Local, external, or NAS libraries — fully offline, no account. macOS + Windows. Link in bio.
#sounddesign #videoediting #audiolibrary #sfx #foley #postproduction #filmmaking #davinciresolve
```

**Reel / Story** — 1080×1920, MP4 H.264+AAC, 15–30 s, content in centre 1080×1420 (top ~250 / bottom ~420 covered).
- ☐ `darkwave-ig-reel-search.mp4` · ☐ `darkwave-ig-story-search.mp4` (same master, §6 9:16 re-frame)

| Time | Visual | Caption |
|---|---|---|
| 0–3s | type in search, results filter live | "stop re-finding the same sound" |
| 3–9s | select clip → Sonic Radar → Find Similar | "it finds what sounds alike" |
| 9–15s | drag to project → typed folders fill | "filed automatically" · end card: alan-design.com |

**IG Ads** — extra crops + field values:
- ☐ `darkwave-ig-ad-feed-01.png` (1080×1350) · ☐ `darkwave-ig-ad-square-01.png` (1080×1080) · ☐ `darkwave-ig-ad-story-01.mp4` (1080×1920)
```
Primary text:  Drop audio in once, find it instantly forever. Real analysis classifies every clip; Sonic Radar finds the ones that sound alike. Offline, no account.
Headline:      Find any sound in seconds
Description:   Local-first sound library for editors
CTA button:    Download
```

### 5.3 X / Twitter
- ☐ `darkwave-x-single-01.png` — 1600×900 (library view, one-liner overlay)
- ☐ `darkwave-x-video-01.mp4` — 1920×1080, ≤2:20 (the §2.3 preview cut)
- ☐ Publish thread:
```
1/ Darkwave is a local-first sound library for editors and sound designers. Drop audio in once, find it instantly forever. macOS + Windows.
2/ It classifies every clip from real decoded-audio analysis — vocal presence, tempo, tonal content — as Soundtrack / Voiceover / SFX / Foley / Ambience. Not filename guessing.
3/ Sonic Radar: "find similar" pulls sounds that actually sound alike, plus instant filters for has-vocals, instrumental, detected tempo, detected pitch.
4/ Point a library at a local disk, external drive, or NAS share. It reconnects on its own and never moves or duplicates your files. Fully offline, no account, no telemetry.
5/ Buy once — alan-design.com  (Mac App Store version coming)
```

### 5.4 YouTube & TikTok

| ✓ | Asset | Size | Length | File |
|---|---|---|---|---|
| ☐ | YouTube walkthrough | 1920×1080 | 3–4 min | `darkwave-yt-walkthrough.mp4` |
| ☐ | YouTube thumbnail | 1280×720 | — | `darkwave-yt-thumb-01.png` (3-word overlay: "Find any sound") |
| ☐ | Shorts / TikTok | 1080×1920 | 20–40 s | `darkwave-short-sonic-radar.mp4` |

**Walkthrough outline:** import a folder → auto-classification + detected tempo/pitch/vocals → search & audition → Sonic Radar find-similar → tag & source/license notes → export to a project's typed folders → NAS reconnect demo → backup.
**Shorts hooks:** "your SFX library doesn't know what's in it" · "not kick_final_v3.wav — actual analysis".
**Audio:** organic Shorts/TikTok may use a trending sound; paid + the App Store preview must be owned/licensed.

---

## 6. Video capture — record once, reuse

**Master:** 2560×1600 · 60 fps · real "Orpheus" library, **no real paths/keys on screen** · 60–120 s · music-free · `darkwave-cap-<feature>-master.mov`.

| ✓ | Master | Covers |
|---|---|---|
| ☐ | `darkwave-cap-library-search-master.mov` | browse, search, audition, waveform scrub |
| ☐ | `darkwave-cap-sonic-radar-master.mov` | analysis filters + Find Similar |
| ☐ | `darkwave-cap-export-master.mov` | select → send to project → typed folder taxonomy |
| ☐ | `darkwave-cap-import-master.mov` | drop folder into watched folder → auto-classify + detect |

| Destination | Size | Length | Format | Captions |
|---|---|---|---|---|
| App Store preview | 1920×1080 | 15–30 s | .mov H.264/ProRes | minimal, app-only |
| Storefront loop | 1600×1000 | 12–20 s | MP4, muted, loop | none |
| Docs embed | 1600×1000 | 8–20 s | MP4+WebM, muted | none |
| Reel / Story / Short / TikTok | 1080×1920 | 15–40 s | MP4 H.264+AAC | burned-in |
| X video | 1920×1080 | ≤2:20 | MP4 H.264+AAC | optional |
| YouTube | 1920×1080 | 3–4 min | MP4 H.264+AAC | reviewed |

Rule: for 9:16, re-frame to the active panel — don't letterbox. Scrub any frame showing a real key, volume name, or path.

---

## 7. Rollup

**Icon** — ☐ regenerate 1024 marketing PNG from the rebaked source ☐ verify no alpha
**Mac App Store** — ☐ 10 text fields ☐ 7 screenshots ☐ preview video ☐ price/age/privacy questionnaire/build attach/submit
**Storefront** — ☐ JSON entry ☐ hero + loop + 5 stills ☐ `appStoreUrl` when MAS live
**Docs** — ☐ 8 screenshots ☐ OG + favicon ☐ links wired with `/darkwave/` prefix ☐ rebuilt + `docs-nginx` restarted
**Social** — ☐ IG carousel (6) + caption ☐ Reel/Story ☐ IG ad crops ☐ X image + video + thread ☐ YT walkthrough + thumb ☐ Short/TikTok
**Masters** — ☐ 4 `darkwave-cap-*-master.mov`

---

## 8. Open decisions

- ☐ Copyright / trading name for the App Store `Copyright` field — `Alan Alves`, `alan-design`, or a registered entity?
- ☐ App Store secondary category — Productivity, or leave single (Music only)?
- ☐ Subtitle — keep `SFX & Music Manager` (19/30) or switch to a benefit line, e.g. `Find any sound in seconds` (25/30)?
- ☐ Store Manager vs direct edit — publish the storefront copy through Store Manager's `manifest.json`, or hand-edit `storeProducts.json` after the final publish so it isn't clobbered?
- ☐ Storefront gallery `image` extension — spec wants `.webp`; the current entry points at `.png`. Confirm the switch and update `products.js` if it references literal filenames.
- ☐ Demo dataset — ship captures from the real "Orpheus" library (scrubbed) or build a small purpose-made demo library?
- ☐ `@alandesign` handle — confirm it exists on IG / X / YouTube / TikTok, or create.
- ☐ Music track for the YouTube walkthrough (owned/licensed).
