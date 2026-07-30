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

The desktop shell wires backup creation: `backup_library` builds a `PortableManifest` from the current asset list (there is no continuously-maintained manifest file yet), writes it beside the catalog, and copies both into a folder the user picks.

Restore is also wired: `restore_library` stages the backup's catalog file next to the live one, swaps the live `Catalog` for an in-memory placeholder under the same mutex every other command uses (which drops the real `Connection` and releases its file handle), keeps a best-effort safety copy of the pre-restore catalog, atomically `rename`s the staged file into place, copies the manifest, and reopens the catalog from disk. Every outcome — including a failed copy or an unreadable restored file — leaves the mutex holding a valid `Catalog`, so later commands never see a missing or poisoned state. This avoids the close-relaunch flow originally anticipated: since the mutex already serializes all catalog access, closing and reopening the connection within the same lock is sufficient, no OS-level app restart required.
