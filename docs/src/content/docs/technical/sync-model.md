---
title: Sync Model
description: Portable manifests and writer leases.
---

The sync model starts with two primitives:

- Portable manifest snapshots with library ID, revision, asset IDs, relative paths, and content hashes.
- Portable manifest file read/write helpers for storing the manifest beside shared media.
- Writer leases with device ID, acquisition time, and TTL.
- Writer lease file read/write helpers, plus acquire and release operations that persist the lease beside the shared media so any device can see the current holder.
- Media-root probes that report online/offline state and whether reconnect validation should run.
- Reconnect validation jobs that expand an online media root and manifest into concrete paths to check.
- Reconnect validation reports that count checked manifest paths and return the missing paths that still need relinking.

The live query database remains local SQLite. Portable manifests are for recovery, reconciliation, and shared-library synchronization, not for live multi-user querying.

If another non-expired device lease exists, the current device should open the shared library as read-only.

The desktop shell wires `probe_media_root` to the active library's real media root (`media_root_status`) instead of a sample path, and exposes `OfflineControlState`'s transitions as a single stateless `apply_offline_control` command: the frontend holds the current state and sends it back with each command, and the command returns the next state. This keeps the actual UseCatalogOnly/RetryReconnect/PauseValidation/ResumeValidation/RelinkMediaRoot decision logic in the tested Rust function rather than reimplemented in the frontend.
