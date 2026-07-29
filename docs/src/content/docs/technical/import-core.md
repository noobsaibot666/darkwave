---
title: Import Core
description: Milestone 1 import and catalog responsibilities.
---

The import core is intentionally small and restart-safe.

`import-pipeline` handles orchestration:

- Validate supported MVP extensions.
- Discover ready watched-folder candidates by comparing current and previous file sizes.
- Maintain watched-folder polling snapshots and drop removed files from the snapshot.
- Ingest platform filesystem notifications with the same stable-size checks used by polling.
- Extract immediate metadata through `audio-metadata`.
- Compute a temporary lightweight content key.
- Register an asset through `storage`.
- Persist pending filename-origin tag suggestions for review.
- Copy managed imports into the library media root after catalog registration.
- Enqueue follow-up jobs.

`storage` handles persistence:

- Opens and migrates the local SQLite catalog.
- Creates and loads libraries.
- Registers assets.
- Suppresses exact duplicate rows by library, content hash, and file size.
- Stores pending jobs.

This preserves the product principle that import should feel immediate while analysis improves the asset progressively in the background.
