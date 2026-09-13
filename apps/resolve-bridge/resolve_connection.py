"""Bootstrap for talking to a running DaVinci Resolve instance.

Mirrors the bootstrap pattern from the team's existing Lua scripting
workspace (`_resolve/_scripts/workspace/shared/resolve.lua`), in Python,
for scripts invoked externally by Darkwave rather than run from inside
Resolve's own Workspace > Scripts menu.

No PyPI dependencies. The native module (`fusionscript`) ships inside the
Resolve installation itself — see README.md for the three environment
variables this relies on and why no separate interpreter or package is
needed.
"""

from __future__ import annotations

import sys


class ResolveNotRunning(RuntimeError):
    """Raised when Resolve's scripting API is reachable but no instance is running."""


class ResolveScriptingUnavailable(RuntimeError):
    """Raised when the fusionscript module itself can't be loaded (env vars, wrong OS path, etc.)."""


def get_resolve():
    """Returns the live Resolve app object, or raises a clear, specific error.

    Callers should catch `ResolveNotRunning` / `ResolveScriptingUnavailable`
    separately from unexpected errors — the former two are expected,
    recoverable states (surface as "open Resolve first" to the user), not
    bugs.
    """
    try:
        import DaVinciResolveScript as dvr  # noqa: N813 (matches Blackmagic's own module name)
    except ImportError as error:
        raise ResolveScriptingUnavailable(
            "Could not import DaVinciResolveScript — check RESOLVE_SCRIPT_API, "
            "RESOLVE_SCRIPT_LIB, and PYTHONPATH are set (see README.md)."
        ) from error

    resolve = dvr.scriptapp("Resolve")
    if resolve is None:
        raise ResolveNotRunning("DaVinciResolveScript loaded, but no running Resolve instance answered.")
    return resolve


def get_current_project():
    """The open project, or raises ResolveNotRunning-style errors from get_resolve()."""
    resolve = get_resolve()
    project = resolve.GetProjectManager().GetCurrentProject()
    if project is None:
        raise ResolveNotRunning("Resolve is running, but no project is currently open.")
    return project


if __name__ == "__main__":
    try:
        project = get_current_project()
    except (ResolveNotRunning, ResolveScriptingUnavailable) as error:
        print(f"[resolve-bridge] {error}", file=sys.stderr)
        sys.exit(1)

    print(f"[resolve-bridge] connected — current project: {project.GetName()}")
