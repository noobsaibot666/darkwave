---
title: Searching
description: Current search and smart collection behavior.
---

Current search behavior supports:

- Text search over asset display names, original filenames, and notes.
- Filtering by accepted tag.
- Filtering by media type.
- Range filters for duration, detected tempo (BPM), and peak level (dB) — shown in a filter panel next to the search bar, and combinable with the text/tag/media-type filters above.
- A Format filter next to the range filters, covering every recognized audio extension (WAV, MP3, FLAC, AAC, M4A, OGG, AIFF — broader than what the decoder itself accepts, so AAC/M4A files stay filterable even though they're not yet decoder-supported), read from each file's own extension rather than a stored field.
- Saving the current search text plus any active range filters as a smart collection.

Smart collections re-run their saved query against the live catalog every time they're opened, rather than storing a fixed list of assets — a smart collection tracks what currently matches, including sounds imported after it was created. They appear in the same sidebar Projects list as regular collections, marked with a small icon; adding assets to one directly isn't possible since its membership is computed, not stored.

Smart import currently uses filename tokens as its first signal. Suggested tags include confidence and origin so the user can review, accept, or reject them.

## Sonic Radar

A sidebar section that turns the app's own automatic analysis into one-click filters, rather than requiring a manual search for each: **Has Vocals** and **Instrumental Only** (from the real Silero VAD speech-detection pass, not a tag guess), and **Detected Tempo** / **Detected Pitch** (whether a BPM or pitch estimate exists at all for a sound yet). These combine with everything else on this page — search text, tags, media type, and the range filters — since they're just another facet of the same underlying catalog.
