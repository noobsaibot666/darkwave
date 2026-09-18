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
cd apps\desktop
npm run tauri dev
```

- `npm install` picks up any new/changed JS dependencies. Safe to run even
  when nothing changed.
- No sidecar rebuild step anymore. Until 2026-09-18 this pull sequence
  included building a `similarity-worker` sidecar binary first (a GPL-3.0
  bliss-rs subprocess "Similar Sounds" depended on) — that crate is gone.
  Its replacement (`crates/audio-analysis::compute_fingerprint`, see
  `docs/adr/0033-drop-bliss-rs-similarity-sidecar.md`) is plain Rust that
  links straight into `darkwave-desktop` like everything else, so "Similar
  Sounds" now works on this Windows build too — it never actually worked
  here before, since a Windows-triple sidecar build was still open work
  (see ADR 0025's own consequences section) that never got done.

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
[Release Readiness](/darkwave/development/release-readiness/). Use this section to
produce a real `.msi`/`.exe` installer for local testing, not to cut an
actual release.

Rebuild the sidecar first if you haven't already this session — packaging
fails immediately without it, same as `tauri dev` does.

Windows has no App Store distribution channel for Darkwave (direct-sale
only — see the distribution plan) — every Windows build is the
`direct-dist` feature, using `tauri.direct.conf.json`'s identifier and the
real licensing/updater plumbing (`src/license.rs`), not the base
sandboxed-for-macOS config. Override `bundle.active` for a single build
without editing the committed config (PowerShell mangles embedded quotes
when they're passed inline to a native `.exe`, so write the override to a
file instead of trying to inline the JSON — this gets merged on top of
`tauri.direct.conf.json`, not in place of it):

```powershell
cd apps\desktop
'{"bundle":{"active":true}}' | Out-File -Encoding utf8 ..\..\bundle-override.json
npx tauri build --features direct-dist --config src-tauri\tauri.direct.conf.json --config ..\..\bundle-override.json
Remove-Item ..\..\bundle-override.json
```

If it succeeds, the installer lands at:

```text
target\release\bundle\nsis\Darkwave_<version>_x64-setup.exe
```

(`msi` isn't produced — `tauri.conf.json`'s `bundle.windows.nsis` block
means NSIS is the configured Windows target, not WiX/MSI.)

**Signing — deliberately skipped for V1.** Decision: Windows ships unsigned,
matching exposeu_wrapkit's (CineFlow Suite) precedent — buyers see a
SmartScreen "unknown publisher" warning on first run. No EV certificate
purchase planned. If that ever changes, this is the process: get an EV
code-signing certificate installed in the Windows certificate store, find
its SHA-1 thumbprint via `certmgr.msc` or `Get-ChildItem Cert:\CurrentUser\My`,
then:

```powershell
& "C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\signtool.exe" sign `
  /sha1 <CERT_THUMBPRINT> `
  /fd SHA256 `
  /tr http://timestamp.digicert.com `
  /td SHA256 `
  target\release\bundle\nsis\Darkwave_<version>_x64-setup.exe
```

This is the one gap the reference distribution pattern (exposeu_wrapkit)
never closed — it shipped Windows builds unsigned. Closing it here is
what flips `signing_notarization`'s Windows half in
[Release Readiness](/darkwave/development/release-readiness/) (the macOS half
still needs a Developer ID identity — see `apps/desktop/scripts/`).

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
| Linker error mentioning `link.exe` or `LINK : fatal error` | VS Build Tools / C++ workload got removed or is out of date | `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| Blank/black window on launch, or a webview-related panic | WebView2 Runtime missing or corrupted | Install from `https://developer.microsoft.com/microsoft-edge/webview2/`, then relaunch |
| `audio-analysis` (or `ort`/`ort-sys`) fails to build, mentioning a download error | No network access at build time for the ONNX Runtime prebuilt binary | Check firewall/proxy/VPN; result is cached in `target\` once it succeeds |
| App launches but the player never picks up a "vocal" mood color | Not a crash — `detect_vocal_ratio` degrades to `None` if the ONNX runtime failed to load at runtime, and the player silently falls back to its tag-only mood guess | Confirm `cargo test --workspace` passed cleanly; if it did, this is cosmetic only |
