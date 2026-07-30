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

The sidebar groups media types as **Soundtracks**, **Sound Effects**, and **Ambience** (Soundtracks is a display label over the same underlying music category — no data changes if you're scripting against it). Sound Effects has an expandable **By category** list of the starter taxonomy's action tags (Impact, Whoosh, Rise, and so on); selecting one filters to just that tag. A **Needs Review** filter also appears for anything import's size-based check flagged as a likely broken or placeholder file.

Creating a project is a single "+ New Project" button in the sidebar, which opens a small dialog for the name rather than an inline field — keeps the sidebar from being cluttered by a text input that's only used occasionally.

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
