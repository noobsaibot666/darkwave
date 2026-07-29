---
title: Offline Behavior
description: Current NAS and offline catalog behavior.
---

Darkwave keeps the active catalog local to the computer. When shared media is unavailable, catalog metadata and search remain usable.

Current behavior:

- Availability validation can mark originals as missing.
- Missing assets remain searchable.
- Relinking an asset updates its path and restores local availability.
- Writer lease state can place a second device into read-only mode while another active writer owns the lease.
- Media-root probing reports whether a NAS-backed root is online or offline.
- Online media-root probes request reconnect validation before assuming asset paths are usable.
- Export plans can wait in a queue while source paths are offline and become ready after the source returns.
- Playback source selection can fall back to cached preview files when originals are missing.
- Reconnect validation jobs are planned from portable manifests when a media root comes back online.
- Reconnect validation scheduling queues one pending job per library revision and marks jobs completed after validation.
- Offline controls support catalog-only mode, reconnect retry, validation pause/resume, and media-root relinking.

Future work will add decoder integration for cached preview playback.
