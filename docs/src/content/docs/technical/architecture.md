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
