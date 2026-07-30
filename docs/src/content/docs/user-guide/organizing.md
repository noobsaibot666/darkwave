---
title: Organizing
description: Current catalog-backed organization behavior.
---

Milestone 3 adds the catalog foundation for organization.

Current behavior supports:

- Starter taxonomy tags such as media types, actions, sources, character, and energy.
- Bulk tag application.
- Project collections.
- Favorite and reviewed states.
- Undo and redo for bulk tag and collection membership changes.
- Browser interaction state for replace selection, range selection, additive selection, and select-all-visible.
- Drag payload targets for tags, collections, projects, favorites, trash, and external export.
- Duplicate review options for keeping, linking, merging metadata, replacing lower-quality versions, or moving duplicates to Trash.
- Trash keeps restore information and requires explicit purge after retention.

Catalog mutations and desktop event wiring remain separate from the interaction-state reducer.
