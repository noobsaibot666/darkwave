---
title: Setup
description: Local development setup.
---

Install Node.js 20 or newer, npm 10 or newer, Rust 1.80 or newer, and the platform prerequisites for Tauri 2. On Windows, follow [Windows Setup & First Run](/development/windows-setup/) instead — it covers the same steps with exact PowerShell commands and the Windows-specific prerequisites (VS Build Tools, WebView2).

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
