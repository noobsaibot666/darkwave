# Mac App Store submission assets

## icon/darkwave-appstore-icon-1024.png
1024×1024, RGB, no alpha — Apple's exact App Store icon spec. Reconstructed
from the app's real icon (via `icon.icns`) with the macOS rounded-corner
mask removed and the underlying gradient extended full-bleed to the edges
(Apple applies its own corner mask on display; a pre-rounded upload isn't
accepted).

## screenshots/
Real captures of the running app (dev build), not mockups. Letterboxed to
exactly 2560×1600 (16:10, Apple's largest accepted macOS size) on a solid
dark backdrop — padded, not cropped, so no real UI content is cut off.

- `darkwave-01-library.png` — main library view (real "Orpheus" library,
  2739 sounds), showing the sound list, SONIC RADAR sidebar, categories,
  and the Tags/Apply Tag panel.
- `darkwave-02-command-palette.png` — the Cmd+K command palette.

Add more here (Editor Workflow, Export, Settings > Release Readiness, etc.)
following the same capture → pad-to-2560×1600 → flatten-no-alpha process
before the actual App Store Connect submission.
