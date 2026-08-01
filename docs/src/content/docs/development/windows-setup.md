---
title: Windows Update Workflow
description: What to run on the Windows machine after every pull — it's already set up and running there, this isn't first-time setup.
---

Darkwave is already cloned, built, and running on this Windows machine.
This page is just what to run after every `git pull`, in a **PowerShell**
terminal — not the one-time prerequisites/clone/first-build steps that got
it running in the first place.

## After every pull

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

- `npm install` picks up any new/changed JS dependencies. Safe to run even
  when nothing changed.
- The sidecar rebuild is required, not optional: `apps\desktop\src-tauri\binaries\`
  is gitignored (it's a compiled, machine-specific artifact), so `git pull`
  never brings a copy along. Skipping it produces
  `resource path ...similarity-worker-x86_64-pc-windows-msvc.exe doesn't exist`
  the moment you try to launch.
- If only `crates/similarity-worker` changed, you can skip straight to just
  the sidecar-rebuild lines rather than the full sequence.

Want to confirm nothing broke before opening the UI?

```powershell
cargo test --workspace
```

Same command CI runs.

## Packaging a distributable build (occasional — not yet release-ready)

Everything above runs the app via `tauri dev`, which never touches
`bundle.active` (`apps/desktop/src-tauri/tauri.conf.json`). It's currently
`false` because no platform has a real packaging pipeline yet — "Windows
platform audit" is still an open gate in
[Release Readiness](/development/release-readiness/). Use this section to
produce a real `.msi`/`.exe` installer for local testing, not to cut an
actual release.

Rebuild the sidecar first if you haven't already this session — packaging
fails immediately without it, same as `tauri dev` does.

Then override `bundle.active` for a single build without editing the
committed config (PowerShell mangles embedded quotes when they're passed
inline to a native `.exe`, so write the override to a file instead of
trying to inline the JSON):

```powershell
cd apps\desktop
'{"bundle":{"active":true}}' | Out-File -Encoding utf8 ..\..\bundle-override.json
npx tauri build --config ..\..\bundle-override.json
Remove-Item ..\..\bundle-override.json
```

If it succeeds, the installers land at:

```text
target\release\bundle\msi\Darkwave_<version>_x64_en-US.msi
target\release\bundle\nsis\Darkwave_<version>_x64-setup.exe
```

**Known gaps this will surface, both already tracked, neither a build
failure:**

- **Vocal detection (Silero VAD) won't work in the installed app.** `ort`
  needs `onnxruntime.dll` at runtime; nothing currently copies it into the
  bundle (`bundle.resources` has no entry for it — see
  `docs/adr/0027-real-vocal-detection-silero-vad.md`).
  This degrades silently by design: the player just falls back to its
  tag-only mood guess instead of crashing. Fixing it for real means either
  adding a `bundle.resources` entry plus setting `ORT_DYLIB_PATH` at
  startup, or switching `ort` to a statically-linked feature — an
  architectural choice for whoever picks up Milestone 7, not something to
  patch ad hoc here.
- **No code signing.** The produced installer is unsigned, so Windows
  SmartScreen will warn "unknown publisher" on first run. Expected until
  the "Signing and notarization" release-readiness gate is addressed —
  ignore it for local testing (click "More info" → "Run anyway").

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `resource path ...similarity-worker-x86_64-pc-windows-msvc.exe doesn't exist` | Sidecar not rebuilt after the last pull (or built for the wrong triple) | Re-run the sidecar-rebuild lines above |
| Linker error mentioning `link.exe` or `LINK : fatal error` | VS Build Tools / C++ workload got removed or is out of date | `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| Blank/black window on launch, or a webview-related panic | WebView2 Runtime missing or corrupted | Install from `https://developer.microsoft.com/microsoft-edge/webview2/`, then relaunch |
| `audio-analysis` (or `ort`/`ort-sys`) fails to build, mentioning a download error | No network access at build time for the ONNX Runtime prebuilt binary | Check firewall/proxy/VPN; result is cached in `target\` once it succeeds |
| App launches but the player never picks up a "vocal" mood color | Not a crash — `detect_vocal_ratio` degrades to `None` if the ONNX runtime failed to load at runtime, and the player silently falls back to its tag-only mood guess | Confirm `cargo test --workspace` passed cleanly; if it did, this is cosmetic only |
