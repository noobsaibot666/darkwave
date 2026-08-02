---
title: Setup
description: Local development setup.
---

Install Node.js 20 or newer, npm 10 or newer, Rust 1.80 or newer, and the platform prerequisites for Tauri 2 (on Windows: Rust via rustup with the `x86_64-pc-windows-msvc` toolchain, VS Build Tools with the C++ workload for `link.exe`, and the WebView2 Runtime — already installed on the project's Windows machine). Once that machine is set up, use [Windows Update Workflow](/darkwave/development/windows-setup/) for the PowerShell commands to run after every pull.

```sh
npm install
npm run check
```

**Before the desktop app will build on any platform**, build the `similarity-worker` sidecar Tauri's `externalBin` config expects — this is not optional, and skipping it fails with `resource path ... doesn't exist`:

```sh
./scripts/build-similarity-worker-sidecar.sh
```

Re-run it whenever `crates/similarity-worker` changes, or on a fresh machine/checkout (the built binary lives in a gitignored directory, so cloning or pulling never brings a copy with it).

Use `npm run dev` for the desktop UI development server and `npm run tauri` for Tauri commands.

Useful targeted Rust checks:

```sh
cargo test -p storage -p import-pipeline -p audio-metadata
cargo test -p audio-engine -p waveform
cargo test -p release-readiness
```
