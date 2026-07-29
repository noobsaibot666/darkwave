---
title: Command Palette
description: Searchable command registry model.
---

The `command-palette` crate defines the default audio workspace actions:

- Import.
- Search.
- Apply tag.
- Add to collection.
- Export.
- Reveal.
- Convert.
- Rescan.
- Open settings.
- Run maintenance.

Commands have stable ids, titles, categories, and keywords. Search ranks title matches first, then keyword matches, then category matches.
