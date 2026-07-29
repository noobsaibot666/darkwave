---
title: Search and Smart Import
description: FTS search, filename parsing, smart collections, and tag suggestions.
---

Milestone 4 adds the first search and smart-import core.

Implemented behavior:

- Filename parsing extracts a cleaned display name, useful tokens, BPM, and musical key hints.
- Filename-origin tag suggestions include tag name, facet, confidence, and origin.
- Embedded metadata fields such as title, genre, and comments can map to the same tag vocabulary with metadata-origin traceability.
- SQLite FTS5 indexes asset display names, original filenames, and notes.
- Catalog search supports text search, tag filters, and media-type filters.
- Smart collections store serialized query definitions.
- Suggested tags can be accepted or rejected.
- A rejected suggestion from the same origin is preserved and not recreated as pending without new evidence.

This keeps search explainable and local-first while leaving natural-language query translation and larger performance benchmarks for a later refinement.
