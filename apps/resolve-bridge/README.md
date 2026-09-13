# Resolve Bridge

External Python scripts, invoked on demand by the Darkwave desktop app's
Rust backend (subprocess, same pattern as the existing GPL similarity-worker
isolation) — not a persistent process, not an Electron Workflow Integration
plugin, not Studio-only. See `docs/development/editor-workflow-section.md`
for where this fits in the app.

## Environment

No PyPI dependencies. Resolve ships its own scripting module (`fusionscript`,
a native extension) — this just needs three environment variables pointing
at it, and any modern system Python 3 (verified against 3.14.5 on macOS):

```bash
export RESOLVE_SCRIPT_API="/Library/Application Support/Blackmagic Design/DaVinci Resolve/Developer/Scripting"
export RESOLVE_SCRIPT_LIB="/Applications/DaVinci Resolve/DaVinci Resolve.app/Contents/Libraries/Fusion/fusionscript.so"
export PYTHONPATH="$PYTHONPATH:$RESOLVE_SCRIPT_API/Modules/"
```

Windows/Linux paths differ — see `DaVinciResolveScript.py` itself (inside
`Modules/`) for the per-OS default install locations it falls back to.

Available on Resolve's free edition, not just Studio — this is the classic
scripting API, unlike the newer Electron-based Workflow Integration plugins.

## Smoke test

With Resolve open and a project loaded:

```bash
python3 resolve_connection.py
```

Prints the current project's name, or a clear error if Resolve isn't
running / no project is open / the environment variables aren't set.

## Status

Environment verified working (import + graceful "not running" handling).
No live-connection test yet — needs Resolve actually open to confirm. Real
bridge logic (project-structure mirroring, send-to-timeline) not started —
see the feature index doc above for what's planned.
