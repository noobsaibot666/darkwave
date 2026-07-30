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
- Cached preview decoding into PCM buffers for offline playback preparation, using native WAV decoding or an attached packaged decoder provider.
- Supported arbitrary-format audio decoding through the `audio-metadata` packaged-decoder provider interface.
- Decode cancellation tokens so a newer asset load invalidates earlier decode work.
- Startup latency classification with a pass threshold at 100 ms.
- Output route selection from saved preferences and available device IDs.
- Output route binding to platform output handles with explicit system-default fallback state.
- Platform transport snapshots that combine session state, playback speed, loop state, and output handle ID for the audio backend.
- Backend-ready PCM playback buffers that apply the selected speed with linear resampling.

Loading a new asset resets position to zero, clears loop state, and stops playback. Playback speed is transport state, so it can be adjusted independently and reset to normal without reloading the selected asset. Decode coordination issues a new token for each load, so moving to another row invalidates previous decode work before it can overlap the current selection.

When an original file is local, playback source selection uses the original media path. When the original is missing or unavailable but a preview cache file exists, the cached preview is selected instead. Cached previews can be decoded into PCM for playback preparation through native WAV decoding or an attached packaged decoder provider. Assets without either source are reported unavailable.

Output routing uses the system default unless the saved preference names a currently available device. Missing saved devices fall back to the system default while preserving which device disappeared. Bound routes carry the platform output handle used by the audio backend; if no default handle is available, the engine reports an unbound fallback instead of pretending playback can start. Platform transport snapshots only become ready when an asset is loaded and a platform output handle is bound. Decoded PCM can then be converted into a platform playback buffer that carries the selected output handle and applies the transport speed before backend submission.

Future Milestone 2 work will connect this state model to shipped arbitrary-format decoder artifacts.

## Shipped desktop playback

The first working playback in the desktop shell does not go through this Rust state machine yet. It plays audio directly through the platform webview's native `<audio>` element: the shell resolves an asset's absolute file path (`asset_playback_path`, joining a managed asset's relative path with its library's media root, or returning a referenced asset's path as-is) and hands it to the frontend as an `asset://` URL via Tauri's asset protocol. Waveform peaks for the currently loaded asset are computed client-side with the Web Audio API's `decodeAudioData`, not from a precomputed cache. This is a pragmatic MVP choice: WebKit's media pipeline already handles WAV/AIFF/MP3/AAC/M4A correctly (OGG is not reliably supported by WebKit and is a known gap), so it avoids needing `rodio`/`cpal` native audio output before a first playable build exists. The `audio-engine` state machine remains available for a future native decode path — for example sample-accurate scrubbing, or formats the webview can't play.
