# Windows developer — notes from macOS

Messages from the macOS dev to the Windows dev. Short entries only.
Reply in [macos-developer-notes.md](macos-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

---

## 2026-09-12 — new: keep-awake during overnight analysis batches (needs your eyes on Windows)

Added `apps/desktop/src-tauri/src/power.rs` + a new autonomous job-drive loop in `lib.rs`'s
`setup()`. Goal: a big analysis batch queued up and left to run overnight shouldn't get cut short
by the OS idle-sleeping the machine. Two platform paths, no new deps:

- macOS: spawns `/usr/bin/caffeinate -i` while there's unpaused pending work, kills it when the
  queue empties/pauses/the window closes. Verified locally (unit tests in `power.rs` spawn a real
  `caffeinate` and confirm it lives/dies correctly) and by watching `ps aux | grep caffeinate`
  during a real analysis batch.
- Windows: calls `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` to engage, back to
  `ES_CONTINUOUS` alone to release — via a hand-rolled `extern "system"` binding to `kernel32`
  (see `power.rs`'s `#[cfg(target_os = "windows")] mod imp`). **I can't verify this from macOS.**

Please sanity-check on Windows after pulling: queue up a handful of pending analysis jobs (import
something, or use a library with a backlog), let it idle a bit while jobs are processing, and
confirm via Task Manager → Details (or `powercfg /requests` in an elevated prompt) that
`darkwave-desktop.exe` is holding an `ES_SYSTEM_REQUIRED` request while jobs are pending, and that
it goes away once the queue drains or you pause analysis in the UI. There's also a new "Keep the
computer awake while analyzing" toggle in Settings → General if you want to confirm the opt-out
path too (should mean no request is ever held even with jobs pending).

`cargo check`/`cargo test`/`cargo clippy` all pass on macOS for both the default and
`direct-dist` feature builds; nothing new needed on the Rust side beyond a pull.

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
