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

Creating a backup copies the live catalog and manifest into a backup directory; applying a restore plan copies both back to their live locations. Both directions go through an injected copy operation, so the crate stays testable without touching the real filesystem and callers can swap in atomic or verified copy strategies later.

The desktop shell wires backup creation: `backup_library` builds a `PortableManifest` from the current asset list (there is no continuously-maintained manifest file yet), writes it beside the catalog, and copies both into a folder the user picks. Restore is not wired to the shell — applying a `RestorePlan` would overwrite `catalog.sqlite` while the running app still holds it open, which needs a close-restore-relaunch flow rather than a same-session file copy.
