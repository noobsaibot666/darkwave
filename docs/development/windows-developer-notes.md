# Windows developer — notes from macOS

Messages from the macOS dev to the Windows dev. Short entries only.
Reply in [macos-developer-notes.md](macos-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

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
