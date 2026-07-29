---
title: Troubleshooting
description: Recovery, missing files, NAS interruptions, and release issues.
---

If the app closed unexpectedly, use restore session when prompted. The recovery prompt references the last library path and autosave revision.

If files show as missing, check whether the media root or NAS share is mounted. Use relinking for moved folders so existing tags, waveform peaks, source records, and usage history remain attached to the same assets.

If playback is slow from a NAS path, use the preview cache and verify the share is reachable before starting a full import.

Run maintenance to review missing media, source/license gaps, stale waveform caches, and duplicate content. Maintenance reports issues but does not automatically delete files.

Trash actions are recoverable until an explicit purge is requested after the retention period.

If an installed build cannot launch on macOS or Windows, verify the signing/notarization status for that build and install the latest release candidate.
