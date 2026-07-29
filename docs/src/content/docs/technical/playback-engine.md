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

Loading a new asset resets position to zero, clears loop state, and stops playback. This models the required rapid browser behavior where moving to another row must cancel the previous sound before the decoder/output layer is attached.

Future Milestone 2 work will connect this state model to decoding, output devices, cancellation handles, playback speed, and measured startup latency.
