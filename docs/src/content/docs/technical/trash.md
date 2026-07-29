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
