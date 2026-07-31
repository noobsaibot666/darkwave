---
title: Getting Started
description: Install, create a library, and import audio.
---

Darkwave is a local-first desktop audio library.

Start by creating or opening a library. Creating one only asks for a name — there's no folder to pick upfront. The first folder you import into it becomes its media root automatically, which is what turns on the Refresh action and NAS-offline detection for that library; until then, both stay off since there's nothing yet to scan or watch. A library can manage copied media, reference files where they already live, or use a hybrid mode.

Use folder import for existing sound libraries and watched Downloads for new files. NAS-backed libraries should keep media on the share while the active catalog and waveform cache stay local.

Keep backups of the library manifest and media root. If a NAS path moves, use relinking instead of re-importing the same files.

Default settings use compact browser rows, the system audio output device, and a 2 GB local playback cache. These defaults favor fast auditioning on large libraries without claiming more disk space than necessary; the cache limit is editable in Settings, and can be cleared on demand with the Purge Cache button there.

The main window has three regions: a library sidebar on the left (smart filters, projects), the sound browser in the center, and a details inspector on the right. Both side panels can be hidden with the toggle in their own header — a thin collapsed strip stays in place as a handle to bring the panel back, and the center browser and transport bar resize to use the freed space.

Undo/Redo, importing a project's license report, and the keyboard shortcut reference all live in the native menu bar (Edit, Library, and Help respectively) rather than as buttons in the workspace, so the canvas stays focused on the sounds themselves.

Settings is organized like a native system-settings panel: a category list on the left (General, Playback, Storage, Appearance, Accessibility) rather than one long scroll. Appearance is where the theme lives — Dark is the standard, default look, with Light and "Match system" as real, persisted alternatives, not just a dark-mode-only app with an unused toggle.
