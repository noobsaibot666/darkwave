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
- Playback speed percentage with a 50%-200% editorial range and a reset-to-normal command.
- Playback source selection between original media and cached preview files.
- Cached WAV preview decoding into PCM buffers for offline playback preparation.
- Decode cancellation tokens so a newer asset load invalidates earlier decode work.
- Startup latency classification with a pass threshold at 100 ms.
- Output route selection from saved preferences and available device IDs.
- Output route binding to platform output handles with explicit system-default fallback state.
- Platform transport snapshots that combine session state, playback speed, loop state, and output handle ID for the audio backend.
- Backend-ready PCM playback buffers that apply the selected speed with linear resampling.

Loading a new asset resets position to zero, clears loop state, and stops playback. Playback speed is transport state, so it can be adjusted independently and reset to normal without reloading the selected asset. Decode coordination issues a new token for each load, so moving to another row invalidates previous decode work before it can overlap the current selection.

When an original file is local, playback source selection uses the original media path. When the original is missing or unavailable but a preview cache file exists, the cached preview is selected instead. Cached WAV previews can be decoded into PCM for playback preparation. Assets without either source are reported unavailable.

Output routing uses the system default unless the saved preference names a currently available device. Missing saved devices fall back to the system default while preserving which device disappeared. Bound routes carry the platform output handle used by the audio backend; if no default handle is available, the engine reports an unbound fallback instead of pretending playback can start. Platform transport snapshots only become ready when an asset is loaded and a platform output handle is bound. Decoded PCM can then be converted into a platform playback buffer that carries the selected output handle and applies the transport speed before backend submission.

Future Milestone 2 work will connect this state model to arbitrary-format decoding.
