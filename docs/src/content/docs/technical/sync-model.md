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

Retrying a reconnect now does real work through the `validate_reconnect` command: it re-checks every asset's actual on-disk presence against the media root and updates `availability_state` accordingly (`storage::validate_media_availability`), then, when the root is back online, builds a manifest from the live asset list and runs `plan_reconnect_validation` + `validate_reconnect_paths` to report exactly which managed relative paths are still missing. There is still no continuously-maintained manifest file on disk — this one is built in memory for the check the same way `backup_library` builds one for a backup.

Writer leases remain unwired. They coordinate concurrent writers across multiple devices sharing one NAS-backed library, and the desktop shell has no device-identity concept yet (no stable per-install ID, no device pairing UI) — wiring lease acquire/release without that would have nothing meaningful to key off of.
