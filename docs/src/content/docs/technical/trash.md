---
title: Trash
description: Non-destructive application Trash model.
---

The `trash` crate models recoverable deletion:

- Asset id.
- Original path.
- Trash timestamp.
- Reason.
- Current Trash state.
- Whether the physical file was deleted.

Restore returns a relink plan to the original path. Purge is allowed only after the retention period and an explicit request.

`storage` persists Trash state in a `trash_items` table and owns the catalog-level behavior the `trash` crate's pure model doesn't: `list_assets` and `search_assets` exclude any asset currently in Trash, so trashing an asset removes it from the browser and search without deleting the catalog row, its tags, or its undo history. `list_trash_items` lists what's recoverable for a library; `restore_asset_from_trash` makes it visible again; `purge_trash_item` deletes the underlying asset row (cascading to its tags, collection membership, and source record) once the retention period and an explicit request are both satisfied, reusing `TrashItem::is_purge_allowed` for that check. The desktop shell wires all four through Tauri commands with a 30-day retention policy.
