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
- Read WAV LIST/INFO embedded fields for title, genre, and comments when present.
- Compute a temporary lightweight content key.
- Register an asset through `storage`.
- Derive an initial media type from filename and embedded metadata smart-import signals.
- Persist pending filename-origin and metadata-origin tag suggestions for review.
- Attach optional source and license context when the caller has it at import time.
- Copy managed imports into the library media root after catalog registration.
- Enqueue follow-up jobs.

`storage` handles persistence:

- Opens and migrates the local SQLite catalog.
- Creates and loads libraries.
- Registers assets.
- Suppresses exact duplicate rows by library, content hash, and file size.
- Stores pending jobs.

This preserves the product principle that import should feel immediate while analysis improves the asset progressively in the background.

The desktop shell's `import_folder` command drives this pipeline directly: it lists the files in a user-chosen folder, calls `import_file` for each supported one, and returns both the imported assets and any per-file failures rather than aborting the whole import on the first error. It defaults to referenced imports so a first run never copies or moves the user's files; managed import is available but not yet the default from the UI. The background jobs enqueued by import are not processed yet — nothing currently consumes the `background_jobs` table.
