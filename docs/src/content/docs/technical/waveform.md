---
title: Waveform
description: Multi-resolution waveform peak generation.
---

Waveform rendering uses precomputed peak data instead of decoding a full audio file for every visible row.

The `waveform` crate currently provides:

- Bounded sample-to-peak generation.
- Peak downsampling that preserves min/max extremes.
- A cache payload with row, inspector, and transport resolutions.

The frontend can render compact rows from the row-level peaks and reserve denser payloads for the inspector and persistent transport.
