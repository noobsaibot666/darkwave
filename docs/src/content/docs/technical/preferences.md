---
title: Preferences
description: Settings and keyboard shortcut model.
---

The `preferences` crate defines the default editorial audio workspace settings:

- Compact browser density.
- System default output device.
- 16 GB preview cache limit.
- Keyboard shortcuts for playback, row navigation, favorites, search, import, export, and the command palette.

Shortcut validation groups bindings by accelerator and rejects conflicts before settings are saved.

Preferences can be loaded from and saved to a JSON settings file. Missing settings files fall back to the editorial defaults, and saved preferences normalize the preview cache limit before writing.

The maintenance boundary consumes the preview cache limit when planning least-recently-used cache eviction candidates. The planner reports candidates and expected bytes freed; removal still requires explicit application action.

The desktop shell exposes default preferences through a Tauri command. The playback engine consumes the saved output-device preference when choosing an output route, binds that route to platform output handles, and falls back to the system default when a saved device or default handle is unavailable.
