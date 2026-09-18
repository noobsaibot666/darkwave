---
title: Setup
description: Local development setup.
---

Install Node.js 20 or newer, npm 10 or newer, Rust 1.80 or newer, and the platform prerequisites for Tauri 2 (on Windows: Rust via rustup with the `x86_64-pc-windows-msvc` toolchain, VS Build Tools with the C++ workload for `link.exe`, and the WebView2 Runtime — already installed on the project's Windows machine). Once that machine is set up, use [Windows Update Workflow](/darkwave/development/windows-setup/) for the PowerShell commands to run after every pull.

```sh
npm install
npm run check
```

Use `npm run dev` for the desktop UI development server and `npm run tauri` for Tauri commands. There's no separate sidecar to build first — "Similar Sounds" (`crates/audio-analysis`'s perceptual fingerprint, see `docs/adr/0033-drop-bliss-rs-similarity-sidecar.md`) is plain Rust that links straight into the main binary, same as everything else here. (This used to require building a `similarity-worker` sidecar binary first, on every machine, before any build — that step no longer exists, on any platform.)

Useful targeted Rust checks:

```sh
cargo test -p storage -p import-pipeline -p audio-metadata
cargo test -p audio-engine -p waveform
cargo test -p release-readiness
```
