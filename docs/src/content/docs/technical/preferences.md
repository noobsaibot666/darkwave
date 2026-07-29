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

The desktop shell exposes default preferences through a Tauri command. Future work will apply output-device changes to the playback backend.
