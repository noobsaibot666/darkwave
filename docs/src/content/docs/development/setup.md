---
title: Setup
description: Local development setup.
---

Install Node.js 20 or newer, npm 10 or newer, Rust 1.80 or newer, and the platform prerequisites for Tauri 2.

```sh
npm install
npm run check
```

Use `npm run dev` for the desktop UI development server and `npm run tauri` for Tauri commands.

Useful targeted Rust checks:

```sh
cargo test -p storage -p import-pipeline -p audio-metadata
cargo test -p audio-engine -p waveform
```
