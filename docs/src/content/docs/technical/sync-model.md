---
title: Sync Model
description: Portable manifests and writer leases.
---

The sync model starts with two primitives:

- Portable manifest snapshots with library ID, revision, asset IDs, relative paths, and content hashes.
- Portable manifest file read/write helpers for storing the manifest beside shared media.
- Writer leases with device ID, acquisition time, and TTL.
- Media-root probes that report online/offline state and whether reconnect validation should run.
- Reconnect validation jobs that expand an online media root and manifest into concrete paths to check.

The live query database remains local SQLite. Portable manifests are for recovery, reconciliation, and shared-library synchronization, not for live multi-user querying.

If another non-expired device lease exists, the current device should open the shared library as read-only.
