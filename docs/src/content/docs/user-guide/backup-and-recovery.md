---
title: Backup and Recovery
description: Library backup and restore expectations.
---

A restorable backup needs:

- Local catalog snapshot.
- Portable library manifest.
- Media root location.

Before restore, Darkwave validates that the catalog snapshot and manifest are available. If the media root moved, restore keeps the catalog identity and uses relinking to reconnect files.
