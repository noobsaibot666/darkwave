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
- Decode supported packaged formats through the decoder provider interface when release decoder artifacts are installed.
- Prepare drag-and-drop payloads for completed rendered WAV exports.
- Validate selected in/out export ranges.
- Record project usage events.
- Generate project source/license report rows from usage and source records, including attribution, restrictions, and receipt paths.
- Render project source/license reports as CSV.
- Warn, rather than block, when source or license context is missing, uncertain, or expired.

Packaged decoder artifacts remain a release-build requirement before compressed source formats can render in installed builds.
