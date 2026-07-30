---
title: Search and Smart Import
description: FTS search, filename parsing, smart collections, and tag suggestions.
---

Milestone 4 adds the first search and smart-import core.

Implemented behavior:

- Filename parsing extracts a cleaned display name, useful tokens, BPM, and musical key hints.
- Filename-origin tag suggestions include tag name, facet, confidence, and origin.
- Natural-language search parsing strips command/filler words and extracts supported media-type filters such as sound effects, ambience, and music.
- Embedded metadata fields such as title, genre, and comments can map to the same tag vocabulary with metadata-origin traceability.
- SQLite FTS5 indexes asset display names, original filenames, and notes.
- Catalog search supports text search, tag filters, and media-type filters using SQL-backed predicates and supporting indexes.
- Storage includes an ignored 100,000-asset search profile test that can be run explicitly with `cargo test -p storage large_catalog_search_profile_exercises_one_hundred_thousand_assets -- --ignored --nocapture`.
- Smart collections store serialized query definitions.
- Suggested tags can be accepted or rejected.
- A rejected suggestion from the same origin is preserved and not recreated as pending without new evidence.

This keeps search explainable and local-first. Natural-language parsing is intentionally conservative before any later ranking or semantic expansion.

The desktop shell's `search_assets` command runs the query through `parse_natural_language_query` before hitting the catalog: an inferred media type becomes a real `AssetSearchQuery` filter, and the cleaned term list becomes the FTS text. A separate `explain_search_query` command exposes the same parse for a live filter-chip row under the search bar, so what got inferred from a query like "sound effects rain forest" is visible, not silent.
