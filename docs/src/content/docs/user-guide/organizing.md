---
title: Organizing
description: Current catalog-backed organization behavior.
---

Milestone 3 adds the catalog foundation for organization.

Current behavior supports:

- Starter taxonomy tags such as media types, actions, sources, character, and energy.
- Bulk tag application.
- Project collections.
- Favorite and reviewed states.
- Undo and redo for bulk tag and collection membership changes.
- Browser interaction state for replace selection, range selection, additive selection, and select-all-visible.
- Drag payload targets for tags, collections, projects, favorites, trash, and external export.
- Duplicate review options for keeping, linking, merging metadata, replacing lower-quality versions, or moving duplicates to Trash.
- Trash keeps restore information and requires explicit purge after retention.

Catalog mutations and desktop event wiring remain separate from the interaction-state reducer.

The sidebar is organized top to bottom as:

- **All Sounds** — the whole library.
- **Favorites** and **Unreviewed**, each with an expandable **By category** breakdown (Soundtracks, Voice, No Voice, Sound FX) — so "my favorite dialogue" or "unreviewed sound effects" is one click, not a manual scan of a flat list.
- **Needs Review** — anything import's size-based check flagged as a likely broken or placeholder file.
- **Categories** — **Soundtracks**, **Sound Effects**, and **Ambience** (Soundtracks is a display label over the same underlying music category — no data changes if you're scripting against it). Sound Effects has its own expandable **By category** list of the starter taxonomy's action tags (Impact, Whoosh, Rise, and so on).
- **Sonic Radar** — see Searching for what these filters mean.
- **Maintenance** — **Missing Files**, at the bottom since it's the one you reach for least often, not day-to-day browsing.

Every row in this list gets a leading icon and lights up in the app's accent color when active or hovered, so the sidebar reads as one consistent system rather than a plain text list. Favorites, Unreviewed, Categories, Sonic Radar, Projects, Release Readiness, and Maintenance are each a real header — icon, bigger label text, and a fold/unfold chevron — with their categories or options nested underneath, collapsible independently of one another.

The main browser rows also color- and icon-code by what a sound actually is — a green music note for instrumental music, a purple mic for music with vocals, an amber waveform mark for sound effects, and a teal wind glyph for ambience — so you can tell tracks apart at a glance while scanning a long list, without opening each one.

Creating a project is a small "+" icon next to the Projects heading in the sidebar, which opens a dialog for the name rather than an inline field — keeps the sidebar from being cluttered by a text input that's only used occasionally.

## Editor Workflow

A dedicated panel, opened from its own button near the top of the sidebar, consolidates the actions you reach for when a sound is ready to leave the library: **Reveal in Finder/Explorer**, **Copy File Path** (also `Cmd/Ctrl+Shift+C`), and **Send to Project** (the same DaVinci Resolve quick-export described in Exporting, plus every other project with an export folder configured). It opens as an animated panel over the main canvas rather than a modal, so the sidebar and inspector stay visible and usable while it's open.

## Managing libraries

Settings → General → Manage Libraries lists every library with **Clean Cache**, **Empty Trash**, and **Delete** actions. All three only ever touch Darkwave's own catalog records (cached preview copies, trash entries, or the whole catalog row) — the audio files at a library's media root are never read, moved, or deleted by any of them.

## Background Activity

A small activity icon next to Refresh Library lights up (a pulsing LED) whenever metadata extraction, audio analysis, or a library sync is actually running, and stays dim otherwise. Clicking it opens a panel with each running task's own progress bar and a plain-language note on why the app stays responsive while it works — background analysis runs off the main thread by design, so it never has to freeze the window to make progress.

## Find Similar Sounds

Once a sound has been through the background audio-analysis pass (see
Importing), its inspector shows a "Find Similar Sounds" button under Detected
Audio Attributes. It compares that sound's similarity feature vector against
every other analyzed sound in the library and filters the browser to the
closest matches — useful for finding variations of a hit, alternates for a
whoosh, or anything with a similar timbral/spectral character, without
relying on tags or filenames matching. If the sound hasn't been analyzed yet
(analysis runs shortly after import, or after a referenced NAS file finishes
caching locally), the button explains that instead of returning nothing.
