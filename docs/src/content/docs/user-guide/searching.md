---
title: Searching
description: Current search and smart collection behavior.
---

Current search behavior supports:

- Text search over asset display names, original filenames, and notes.
- Filtering by accepted tag.
- Filtering by media type.
- Range filters for duration, detected tempo (BPM), and peak level (dB) — shown in a filter panel next to the search bar, and combinable with the text/tag/media-type filters above.
- Saving the current search text plus any active range filters as a smart collection.

Smart collections re-run their saved query against the live catalog every time they're opened, rather than storing a fixed list of assets — a smart collection tracks what currently matches, including sounds imported after it was created. They appear in the same sidebar Projects list as regular collections, marked with a small icon; adding assets to one directly isn't possible since its membership is computed, not stored.

Smart import currently uses filename tokens as its first signal. Suggested tags include confidence and origin so the user can review, accept, or reject them.
