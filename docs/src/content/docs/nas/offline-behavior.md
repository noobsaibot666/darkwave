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

Future work will add physical NAS probing, preview cache playback, queued exports, reconnect validation jobs, and user-facing offline status.
