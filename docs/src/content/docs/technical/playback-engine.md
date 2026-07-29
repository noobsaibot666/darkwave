---
title: Playback Engine
description: Milestone 2 playback state and future audio output responsibilities.
---

The first playback core is a tested state machine, not the final output engine.

`audio-engine` currently tracks:

- Active asset ID.
- Duration.
- Current position.
- Playing or paused state.
- Optional loop region.
- Playback source selection between original media and cached preview files.
- Output route selection from saved preferences and available device IDs.

Loading a new asset resets position to zero, clears loop state, and stops playback. This models the required rapid browser behavior where moving to another row must cancel the previous sound before the decoder/output layer is attached.

When an original file is local, playback source selection uses the original media path. When the original is missing or unavailable but a preview cache file exists, the cached preview is selected instead. Assets without either source are reported unavailable.

Output routing uses the system default unless the saved preference names a currently available device. Missing saved devices fall back to the system default while preserving which device disappeared.

Future Milestone 2 work will connect this state model to decoding, platform output handles, cancellation handles, playback speed, and measured startup latency.
