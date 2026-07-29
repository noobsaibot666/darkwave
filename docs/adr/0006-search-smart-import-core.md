# ADR 0006: Search and smart import core

## Status

Accepted

## Context

Milestone 4 requires FTS search, faceted filters, saved smart collections, filename parsing, embedded metadata mapping, suggested tags with confidence, and tag approval or rejection.

## Decision

Implement the first search and smart-import core across `search` and `storage`:

- `search` parses filenames into display names, tokens, BPM, key, and traceable filename-origin tag suggestions.
- `storage` owns persisted FTS indexing through SQLite FTS5.
- `storage` evaluates text search, tag filters, and media-type filters over catalog assets.
- Smart collections store serialized visible query definitions.
- Suggested tags use the existing `asset_tags` relationship with confidence, origin, and approval state.
- Rejected suggestions are preserved and not immediately recreated by the same origin.

## Consequences

- Search behavior is local-first and explainable.
- Smart collection query definitions can later be rendered as visible filter chips.
- Filename intelligence remains a first signal, not the final classifier.
- Embedded WAV metadata can contribute traceable metadata-origin suggestions alongside filename signals.
- Natural-language query parsing and large-catalog performance benchmarks remain future Milestone 4 work.
