---
title: Backup
description: Backup package and restore validation model.
---

The `backup` crate models restoreable library backup packages:

- Library id.
- Manifest revision.
- Media root.
- Catalog snapshot path.
- Portable manifest path.
- Creation timestamp.

Restore validation requires both the catalog snapshot and portable manifest. The restore plan preserves the original media root so NAS-backed libraries can be reconnected rather than re-imported.
