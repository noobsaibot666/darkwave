---
title: Architecture
description: Desktop, Rust core, storage, and documentation architecture.
---

Darkwave uses Tauri 2 for the desktop shell, React and TypeScript for the interface, and Rust crates for the local-first core.

The active catalog remains local to each computer. Shared media and portable manifests can live on local disks, external disks, or NAS shares.

Core modules are split by responsibility so indexing, audio analysis, waveform generation, storage, search, synchronization, playback, and export can be tested independently.
