# ADR 0005: Organization core in the local catalog

## Status

Accepted

## Context

Milestone 3 requires tags, starter taxonomy, collections, projects, bulk actions, favorites, review states, and undo. The interaction test matrix also requires redo. These operations are catalog metadata changes and need to remain local-first and recoverable.

## Decision

Keep organization primitives in the `storage` catalog boundary:

- Tags and starter taxonomy.
- Asset-tag relationships with origin and accepted approval state.
- Manual, smart, and project collections.
- Collection membership.
- Favorite and review state flags on assets.
- Undo records for bulk tag application and collection membership, with redo replay for applied relationship undos.

## Consequences

- UI drag targets and shortcuts can call stable catalog operations.
- Bulk organization can be tested without the desktop UI.
- Undo and redo are implemented first for relationship changes; richer metadata snapshot history can extend the same `undo_actions` table later.
- AI-suggested tags can reuse the same asset-tag relationship with different origins and approval states in later milestones.
