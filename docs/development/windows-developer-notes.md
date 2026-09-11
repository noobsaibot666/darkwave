# Windows developer — notes from macOS

Messages from the macOS dev to the Windows dev. Short entries only.
Reply in [macos-developer-notes.md](macos-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

---

## 2026-09-11 — main synced again, pull now (bigger batch)

`main` updated: `918e5b5` → `838d32d` — a long session, several real
backend bugs + hardening. Pull it:

```
git pull origin main
```

No new deps, no new models. Code + one config change only.

**One thing that actually needs your eyes: the CSP.** `tauri.conf.json`'s
`security.csp` went from `null` to a real policy. I derived it from
Tauri's own source for both platforms (macOS uses bare `asset://` /
`ipc://` schemes, Windows resolves to `http://asset.localhost` /
`http://ipc.localhost` since `useHttpsScheme` isn't set) and included
both forms — but I could only live-verify playback on macOS today
(selected a track, confirmed the waveform rendered and the clock
advanced). Please do the same basic check on Windows after pulling:
select a track, hit play, confirm it actually plays and the waveform
renders. If it's silently broken there, it's almost certainly this CSP
— log it in macos-developer-notes.md and I'll adjust.

What else changed (context, not action items):

- Several real job-queue bugs fixed (silent stuck jobs on pause,
  premature job-reset, a stuck-forever fingerprinting step decoupled
  from job completion, VAD reloading its model per file)
- `[profile.release]` added (LTO + single codegen unit + stripped) —
  release builds should be a genuinely faster binary now; also means
  release builds take longer to compile than before (verified: ~2min
  for darkwave-desktop alone on this Mac)
- Orphaned preview-cache files now cleaned up on trash deletion
- Dropped an unused npm dependency, fixed an npm audit advisory

Action for you: pull, rebuild (release build will be slower now — see
above, that's expected), confirm playback works. Flag anything
Windows-specific below.

---

## 2026-09-11 — main synced, pull now

`main` updated: `db9147b` → `918e5b5`.

Pull it:

```
git pull origin main
```

No new deps. No new Rust crates. No model files needed. Code only.

What changed (for context, not action items):
- Tag suggestions, project drag-and-drop, project edit icon
- Sonic Radar Analyze buttons fixed (Pitch/Instruments were silently failing)
- Waveform-generation pause toggle + lower concurrency
- 3 backend bugs fixed that stuck instrument-detection jobs in "processing" forever

Action for you: just pull, rebuild, confirm it launches and a track selects
without hanging. No Windows-specific work expected here — flag it below if
something breaks only on Windows.
