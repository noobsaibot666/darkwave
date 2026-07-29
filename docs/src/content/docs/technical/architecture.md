---
title: Architecture
description: Desktop, Rust core, storage, and documentation architecture.
---

Darkwave uses Tauri 2 for the desktop shell, React and TypeScript for the interface, and Rust crates for the local-first core.

The active catalog remains local to each computer. Shared media and portable manifests can live on local disks, external disks, or NAS shares.

Core modules are split by responsibility so indexing, audio analysis, waveform generation, storage, search, synchronization, playback, and export can be tested independently.

Milestone 1 introduces the first catalog boundary:

- `storage` owns the local SQLite catalog, library records, asset records, duplicate suppression, availability state, and persistent background jobs.
- `audio-metadata` extracts immediate filesystem metadata without decoding audio.
- `import-pipeline` turns a file path into a catalog asset and job records while avoiding incomplete watched-folder downloads.
- `audio-engine` owns playback session state so rapid row changes cancel prior playback before decoder integration.
- `waveform` creates reusable peak payloads for row, inspector, and transport renderers.
- `storage` also owns organization primitives: tags, asset tags, collections, projects, favorite/review flags, and undo records.
- `search` provides filename intelligence and traceable suggestions, while `storage` owns FTS-backed catalog search and smart collection persistence.
- `library-sync` owns portable manifests and writer leases; `storage` owns local availability validation and relinking.
- `export-pipeline` owns non-destructive editorial export plans; `storage` owns usage events and project source/license report data.
- `release-readiness` owns accessibility policy, crash-recovery prompts, and release gate status.
- `preferences` owns user settings defaults, shortcut bindings, and shortcut conflict validation.
- `workspace-state` owns browser focus, selection, range/additive selection, and drag payload state.
- `viewport` owns virtualized browser row range and spacer calculations for large result sets.
- `command-palette` owns searchable action metadata for palette, shortcut, and menu commands.
