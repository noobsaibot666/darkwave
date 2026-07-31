---
title: Exporting
description: Current editorial export behavior.
---

Current export behavior is planning and traceability focused.

Darkwave can:

- Plan a copy of the original file into a project media folder.
- Plan a WAV 48 kHz/24-bit editorial copy.
- Execute original-file copy exports without changing the library original.
- Queue exports whose source paths are temporarily offline.
- Prepare external drag-and-drop payloads for ready original-copy exports.
- Render decoded PCM to 24-bit WAV files with resampling and selected range slicing.
- Decode any MVP-supported source format (WAV, MP3, FLAC, AAC, M4A, OGG, AIFF) through a real Symphonia-backed decoder — no separate release-only decoder artifact is required; this works the same in every build.
- Offer a WAV (24-bit) conversion path via the command palette's "Convert to WAV" action, alongside the default original-file copy behind Export Selected.
- Prepare drag-and-drop payloads for completed rendered WAV exports.
- Validate selected in/out export ranges.
- Record project usage events.
- Generate project source/license report rows from usage and source records, including attribution, restrictions, and receipt paths.
- Render project source/license reports as CSV.
- Warn, rather than block, when source or license context is missing, uncertain, or expired.

## Quick-export to DaVinci Resolve

Each project can be given an export folder (set at creation, or later in Settings). Once configured, a single "DR" button — in the transport, in the project's row in the sidebar, and in the Editor Workflow panel (see Organizing) — copies the selected sound(s) straight into that folder, so they're one drag away from the timeline in Resolve. This is a one-click copy to a watched folder, not live Resolve scripting automation; it uses the same original-copy export path as the rest of this page, just with the destination pre-configured per project.

## Reveal and copy path

The Editor Workflow panel also has **Reveal in Finder/Explorer** and **Copy File Path** (`Cmd/Ctrl+Shift+C`) for the current selection — the two simplest ways to hand a sound to another application by hand when a project export folder isn't set up.
