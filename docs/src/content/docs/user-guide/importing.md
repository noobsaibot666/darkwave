---
title: Importing
description: Current import behavior for the first library core milestone.
---

Milestone 1 supports the first backend import path.

When a supported audio file is imported, Darkwave:

- Reads immediate file metadata such as extension and file size.
- Reads WAV embedded title, genre, and comment fields when present.
- Registers the asset in the local catalog.
- Records whether the asset is managed or referenced.
- Copies managed imports into the library media root under `Media/00`.
- Sets an initial media type when filename or embedded metadata clearly implies music, ambience, or sound effects.
- Tracks the asset as locally available.
- Avoids creating a second asset row when the same content hash and file size already exist in the same library.
- Adds pending filename-based and metadata-based tag suggestions for review.
- Can attach known source URL, provider, license, attribution, restrictions, and receipt context at import time.
- Creates pending background jobs for embedded metadata extraction and waveform generation, processed automatically right after import and during playback respectively.
- Discovers watched-folder candidates only after supported audio files have stabilized.
- Maintains watched-folder size snapshots across polls so new files are imported only after a later stable scan.
- Ingests platform filesystem notifications and emits import candidates only after repeat stable events.
- Decodes WAV PCM files for cached-preview playback preparation.
- Supports a packaged-decoder provider interface for compressed MVP formats once release decoder artifacts are installed.
- Reports codec capability for imported extensions: native WAV PCM, packaged-decoder formats, or unsupported with conversion available.

Watched folders ignore incomplete browser download files such as `.crdownload`, `.download`, `.part`, and `.tmp`. A watched file is considered ready only after its file size has stabilized.

Referenced imports remain catalog-only and keep pointing at their original file path. Arbitrary-format decoder artifacts and licensing review remain release-build requirements.

## Folder import is recursive

Choosing a folder to import walks every subfolder underneath it — real libraries are rarely flat, and a folder full of vendor/pack subfolders imports in one pass. Dotfiles and dot-directories (`.DS_Store`, `.git`, and similar) are skipped. Recognized audio extensions are intentionally broader than what the app can natively decode yet (adding formats like WMA, CAF, WV, APE, and Opus on top of the core WAV/AIFF/MP3/FLAC/AAC/OGG set): a file can be cataloged, tagged, and organized before Darkwave can play it.

## Smart categorization on import

Two size-based checks run automatically as part of import, before any manual tagging:

- Files at or below roughly 8 KB are routed to a **Needs Review** category instead of being classified normally — that size is far too small to be real audio and typically means a placeholder, a sync stub, or a truncated download. The file is still imported and visible, just flagged.
- Files at or below roughly 5 MB that filename and embedded-metadata analysis didn't already classify default to **Sound Effect** rather than a generic "other" category, since a real music track is essentially never that small.

Both are available as smart filters in the sidebar.

## Refresh and the local cache

The Refresh action (top of the workspace) re-scans the active library's media root for files that aren't in the catalog yet — useful after dropping something into a NAS folder outside the app. It only reads and hashes files it doesn't already know about, so repeat scans are fast regardless of library size.

Separately, opening a library warms a local playback cache (referenced/NAS-backed files only) up to the budget set in Settings, so recently-added sounds preview quickly. The cache is cleared automatically when the app closes, or on demand via the Purge Cache button in Settings.
