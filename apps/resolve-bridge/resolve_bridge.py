"""On-demand Resolve Live Bridge actions, invoked as a subprocess by the
desktop app's Rust backend (see apps/desktop/src-tauri/src/lib.rs,
`sync_project_to_resolve`/`send_asset_to_resolve_timeline`) — one process per
call, not a resident worker (see README.md for why).

Protocol: one JSON object on stdin, e.g.
    {"action": "sync_structure", "project_name": "...", "roles": ["music", "foley"]}
    {"action": "send_to_timeline", "project_name": "...", "role": "music", "file_path": "..."}
One JSON object on stdout, always shaped `{"success": true, ...}` or
`{"success": false, "error": "..."}`. Exit code mirrors `success` so the Rust
side can branch on either without re-parsing the body if it just needs the
outcome.

Usage: python3 resolve_bridge.py < request.json
"""

from __future__ import annotations

import json
import sys

from resolve_connection import ResolveNotRunning, ResolveScriptingUnavailable, get_current_project

# `role` vocabulary matches `crates/storage/src/lib.rs`'s `project_export_folders`
# comment exactly (music/sound_effect/voiceover/foley/ambience/documents) —
# `documents` is deliberately absent here, same as it's excluded from audio
# export routing everywhere else in the app.
ROLE_BIN_NAMES = {
    "music": "Music",
    "sound_effect": "SFX",
    "voiceover": "VO",
    "foley": "Foley",
    "ambience": "Ambience",
}


def find_subfolder(folder, name):
    for sub in folder.GetSubFolderList():
        if sub.GetName() == name:
            return sub
    return None


def ensure_subfolder(media_pool, parent_folder, name):
    """Idempotent — safe to call every time structure sync runs, not just
    the first time. A project's bin (and its role sub-bins) should exist
    exactly once no matter how many times sync fires."""
    existing = find_subfolder(parent_folder, name)
    if existing is not None:
        return existing
    return media_pool.AddSubFolder(parent_folder, name)


def sync_structure(payload):
    project = get_current_project()
    media_pool = project.GetMediaPool()
    root = media_pool.GetRootFolder()

    project_name = payload["project_name"]
    roles = payload.get("roles", [])

    project_bin = ensure_subfolder(media_pool, root, project_name)
    bins_created = []
    for role in roles:
        bin_name = ROLE_BIN_NAMES.get(role)
        if bin_name is None:
            continue
        ensure_subfolder(media_pool, project_bin, bin_name)
        bins_created.append(bin_name)

    return {"success": True, "project_bin": project_name, "bins": bins_created}


def send_to_timeline(payload):
    project = get_current_project()
    media_pool = project.GetMediaPool()
    root = media_pool.GetRootFolder()

    project_name = payload["project_name"]
    role = payload["role"]
    file_path = payload["file_path"]

    project_bin = find_subfolder(root, project_name)
    if project_bin is None:
        raise RuntimeError(
            f"no Resolve bin found for project '{project_name}' — run structure sync first"
        )

    bin_name = ROLE_BIN_NAMES.get(role)
    target_bin = (find_subfolder(project_bin, bin_name) if bin_name else None) or project_bin

    media_pool.SetCurrentFolder(target_bin)

    # Reuse an already-imported clip with the same source path instead of
    # importing a duplicate every time the same sound gets sent again.
    media_pool_item = None
    for clip in target_bin.GetClipList():
        if clip.GetClipProperty("File Path") == file_path:
            media_pool_item = clip
            break

    if media_pool_item is None:
        imported = media_pool.ImportMedia([file_path])
        if not imported:
            raise RuntimeError(f"Resolve could not import {file_path}")
        media_pool_item = imported[0]

    timeline = project.GetCurrentTimeline()
    if timeline is None:
        raise RuntimeError("no timeline is open in the current Resolve project")

    placed_at_playhead = _append_at_playhead(media_pool, timeline, media_pool_item)
    return {
        "success": True,
        "clip": media_pool_item.GetName(),
        "placed_at_playhead": placed_at_playhead,
    }


def _append_at_playhead(media_pool, timeline, media_pool_item):
    """Best-effort placement at the current playhead position. Falls back to
    a plain append (end of the appropriate track — guaranteed to work) if
    anything about computing or using the playhead frame fails; this is a
    nice-to-have; a working append is not optional. Not yet verified against
    a live Resolve instance — see docs/development/editor-workflow-section.md
    and this file's own comment history for the fallback-first reasoning.

    Verified live against a real Resolve instance: `GetCurrentTimecode()`
    already returns an *absolute* timeline position (Resolve timelines
    start at 01:00:00:00 by convention, not 00:00:00:00), and that absolute
    value is exactly what `recordFrame` expects. An earlier version of this
    function added `timeline.GetStartFrame()` on top of the converted
    timecode, double-counting that hour offset — confirmed by a live test
    that placed a clip at 02:00:00:00 instead of the actual playhead
    position (01:00:00:00). Do not add `GetStartFrame()` back in.
    """
    try:
        frame_rate = float(str(timeline.GetSetting("timelineFrameRate")).split()[0])
        record_frame = _timecode_to_frame_offset(timeline.GetCurrentTimecode(), frame_rate)
        result = media_pool.AppendToTimeline(
            [{"mediaPoolItem": media_pool_item, "recordFrame": record_frame}]
        )
        if result:
            return True
    except Exception:
        pass
    media_pool.AppendToTimeline([media_pool_item])
    return False


def _timecode_to_frame_offset(timecode, frame_rate):
    """Converts an absolute Resolve timecode string to an absolute frame
    number — matches what `recordFrame` expects directly, no further
    adjustment against the timeline's start frame needed (see
    `_append_at_playhead`'s doc comment)."""
    hours, minutes, seconds, frames = (int(part) for part in timecode.replace(";", ":").split(":"))
    return int(round((hours * 3600 + minutes * 60 + seconds) * frame_rate)) + frames


ACTIONS = {
    "sync_structure": sync_structure,
    "send_to_timeline": send_to_timeline,
}


def main():
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(json.dumps({"success": False, "error": f"invalid JSON on stdin: {error}"}))
        sys.exit(1)

    action = payload.get("action")
    handler = ACTIONS.get(action)
    if handler is None:
        print(json.dumps({"success": False, "error": f"unknown action: {action!r}"}))
        sys.exit(1)

    try:
        result = handler(payload)
    except (ResolveNotRunning, ResolveScriptingUnavailable) as error:
        print(json.dumps({"success": False, "error": str(error)}))
        sys.exit(1)
    except (KeyError, RuntimeError) as error:
        print(json.dumps({"success": False, "error": str(error)}))
        sys.exit(1)

    print(json.dumps(result))
    sys.exit(0 if result.get("success") else 1)


if __name__ == "__main__":
    main()
