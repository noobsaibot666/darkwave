---
title: Windows Setup & First Run
description: Sequential PowerShell walkthrough for building and running Darkwave on Windows for the first time.
---

Follow these in order, in a **PowerShell** terminal. Each step assumes the previous one succeeded. This is the Windows counterpart to [Setup](/development/setup/); run it once per machine, then just the "Pull and rebuild" section at the bottom on every later visit.

## 1. One-time prerequisites

Install these first if you haven't already. `winget` (built into Windows 11 and modern Windows 10) covers all of them:

```powershell
winget install --id Git.Git -e
winget install --id OpenJS.NodeJS.LTS -e
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

- **Git** — to clone/pull the repo.
- **Node.js** (LTS, currently 22.x) — `node` must be `>=20.11`, `npm` must be `>=10` (checked by `package.json`'s `engines` field).
- **Rust via rustup** — installs the `x86_64-pc-windows-msvc` toolchain by default, which is what this project targets on Windows. Minimum `1.80` (see `Cargo.toml`'s `rust-version`).
- **VS Build Tools with the C++ workload** — required to *link* Rust binaries on Windows (the MSVC toolchain needs `link.exe` and the Windows SDK). Without this, `cargo build` fails with a linker error, not a Rust error.

**WebView2 Runtime** — Tauri's Windows webview. Windows 11 and current Windows 10 builds already have it; if a later step complains about a missing webview, install it from Microsoft's Evergreen installer (`https://developer.microsoft.com/microsoft-edge/webview2/` — search "WebView2 Runtime" if the direct link has moved) and re-run.

Close and reopen your PowerShell window after installing so `git`, `node`, `npm`, `cargo`, and `rustc` are all on `PATH`. Verify:

```powershell
git --version
node --version
npm --version
rustc --version
cargo --version
```

## 2. Get the code

First time on this machine:

```powershell
git clone https://github.com/noobsaibot666/darkwave.git
cd darkwave
```

## 3. Install JS dependencies

```powershell
npm install
```

## 4. Build the similarity-worker sidecar

**Required before the app will build at all** — Tauri's `externalBin` config (`apps/desktop/src-tauri/tauri.conf.json`) expects a platform-named binary to already exist. Skipping this produces `resource path ... doesn't exist`, the exact error that was breaking CI until it was fixed to run this same step.

```powershell
cargo build -p similarity-worker --release --manifest-path .\Cargo.toml
$triple = (rustc -vV | Select-String '^host:').Line.Split(' ')[1]
New-Item -ItemType Directory -Force -Path apps\desktop\src-tauri\binaries | Out-Null
Copy-Item "target\release\similarity-worker.exe" "apps\desktop\src-tauri\binaries\similarity-worker-$triple.exe" -Force
Write-Host "Wrote apps\desktop\src-tauri\binaries\similarity-worker-$triple.exe"
```

That's a PowerShell port of `scripts/build-similarity-worker-sidecar.sh`. If you have Git Bash available (it ships with Git for Windows from step 1), you can run the original script instead and skip retyping this:

```powershell
bash scripts/build-similarity-worker-sidecar.sh
```

Either way, re-run this step whenever `crates/similarity-worker` changes.

## 5. Verify the build

```powershell
cargo test --workspace
```

This is the same command CI runs. It also exercises the new Silero VAD dependency (`ort`/`voice_activity_detector`, in `crates/audio-analysis`) for the first time on this machine — its build script downloads a prebuilt ONNX Runtime binary, so this step needs outbound internet access the first time. If your network blocks that, the audio-analysis crate won't build; everything else will. CI's `windows-latest` job already confirmed this dependency builds cleanly, so a failure here points at local network/proxy/firewall settings, not the code.

## 6. Launch the app

```powershell
cd apps\desktop
npm run tauri dev
```

First launch compiles the whole Rust workspace in dev mode, so expect it to take a few minutes. Subsequent launches are much faster (incremental compilation). The app window should open once you see Vite's dev server ready message followed by the Tauri window appearing.

## Pull and rebuild (every time after the first)

```powershell
cd darkwave
git pull origin main
npm install
cargo build -p similarity-worker --release --manifest-path .\Cargo.toml
$triple = (rustc -vV | Select-String '^host:').Line.Split(' ')[1]
Copy-Item "target\release\similarity-worker.exe" "apps\desktop\src-tauri\binaries\similarity-worker-$triple.exe" -Force
cd apps\desktop
npm run tauri dev
```

`apps\desktop\src-tauri\binaries\` is gitignored (it's a compiled, machine-specific artifact), so `git pull` never brings a stale or foreign-platform copy — rebuilding it after every pull is cheap and keeps it correct, not optional busywork.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `resource path ...similarity-worker-x86_64-pc-windows-msvc.exe doesn't exist` | Sidecar not built yet (or built for the wrong triple) | Re-run step 4 |
| Linker error mentioning `link.exe` or `LINK : fatal error` | VS Build Tools / C++ workload missing | Re-run the `winget install ... VisualStudio.2022.BuildTools` command from step 1, make sure `--add Microsoft.VisualStudio.Workload.VCTools` is included |
| Blank/black window on launch, or a webview-related panic | WebView2 Runtime missing | Install the WebView2 Runtime (see step 1), then relaunch |
| `audio-analysis` (or `ort`/`ort-sys`) fails to build, mentioning a download error | No network access at build time for the ONNX Runtime prebuilt binary | Check firewall/proxy/VPN; this only needs to succeed once, the result is cached in `target\` |
| App launches but the player never picks up a "vocal" mood color | Not a crash — `detect_vocal_ratio` degrades to `None` if the ONNX runtime failed to load at runtime, and the player silently falls back to its tag-only mood guess | Confirm step 5 passed cleanly; if it did, this is cosmetic only |
