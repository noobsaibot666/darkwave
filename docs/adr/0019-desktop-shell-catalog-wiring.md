# ADR 0019: Desktop shell catalog wiring

## Status

Accepted.

## Context

Through Milestone 5, `storage`, `import-pipeline`, and the other domain crates were fully implemented and tested but never called from the Tauri shell. Every command in `apps/desktop/src-tauri` returned sample or hardcoded data, and the frontend rendered a static mockup. No user could actually create a library, import a file, or see a real asset. The developer build instruction (plan §26) prioritizes a working vertical slice — create a library, import a folder, browse it — above further domain-logic work.

## Decision

Wire the desktop shell to the real catalog instead of adding new sample commands:

- On startup, open (and migrate) a SQLite catalog at `catalog.sqlite` inside the OS app data directory, held as managed Tauri state behind a `Mutex`.
- Replace the sample Tauri commands with real ones backed by `storage::Catalog` and `import_pipeline::import_file`: `list_libraries`, `create_library`, `list_assets`, `search_assets`, `import_folder`.
- `import_folder` walks a chosen directory non-recursively, imports each supported file, and reports imported assets plus per-file failures instead of aborting on the first error.
- The frontend's `App.tsx` calls these commands directly: an onboarding screen collects a library name and media root (via the native folder picker) when no library exists yet, and the asset browser renders real `AssetRecord` rows instead of the previous static array.
- `storage::LibraryRecord`, `AssetRecord`, and `AssetPath` now derive `Serialize`/`Deserialize` so they can cross the Tauri IPC boundary directly, avoiding parallel DTO structs.

Deliberately out of scope for this pass: audio playback (the `audio-engine` crate has no real output device dependency yet — no `rodio`/`cpal`), background processing of the job queue rows that `import_file` already enqueues, waveform rendering, and wiring the remaining inspector panels (tags, source/license, settings) to real data. `import_folder` defaults its mode to `referenced` so a first import never copies or moves a user's files.

## Consequences

- The plan's first usable vertical slice now partially exists: a user can create a library, import a folder, and see real files with real metadata in the browser.
- The Tauri command surface takes `String` UUIDs and paths at the boundary and converts internally, so IPC argument types stay plain JSON-friendly values.
- Because `import_file` was already fully implemented and tested in `import-pipeline`, this pass added no new import logic — only the plumbing to call it.
- Playback, background job processing, and full inspector wiring remain the next concrete gaps toward a complete vertical slice.
