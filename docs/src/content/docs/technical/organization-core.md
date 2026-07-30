---
title: Organization Core
description: Tags, collections, projects, review state, undo, and redo in the local catalog.
---

The organization core lives in the local SQLite catalog.

Implemented catalog behavior:

- Seed starter taxonomy terms once.
- Create tags with normalized names and facets.
- Apply a tag to multiple assets in one operation.
- List tags for an asset.
- Create manual, smart, and project collections.
- Add multiple assets to a collection.
- List assets in a collection.
- Mark assets as favorite or reviewed.
- Undo bulk tag application and collection membership operations.
- Redo bulk tag application and collection membership operations after undo.

This supports the future UI model where selected assets can be dragged onto tags, collections, projects, categories, and quick targets without opening dialogs.
