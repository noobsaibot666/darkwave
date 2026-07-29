---
title: Export Pipeline
description: Non-destructive export planning and traceability.
---

The export core currently plans editorial exports without touching the library original.

Implemented behavior:

- Original-file export plans.
- WAV 48 kHz/24-bit conversion plans.
- Selected in/out range validation.
- Destination path planning for project media folders.
- Non-destructive original-file copy execution.
- Decoded-PCM WAV rendering for 24-bit exports, including linear resampling and selected range slicing.
- Queued export state for offline source paths and ready original-copy execution.
- External drag payloads for ready original-copy exports, including ordered file URLs and license-report intent.
- External drag payloads for completed rendered WAV exports.
- Traceability flags for preserving source and license records.
- Project source reports include attribution, restriction notes, and source receipt paths.
- CSV rendering for project source/license reports.
- License assessment warnings for missing, uncertain, or expired source/license context.

The planner remains separate from execution. Original-file copies can be executed directly, held in a queue until offline source paths become available, or exposed as OS-facing drag payloads after the destination copy exists. WAV exports can be rendered once a decoder supplies PCM; the renderer resamples to the target rate, writes 24-bit WAV data, applies selected ranges, and exposes completed render outputs as external drag payloads. Arbitrary-format decoder attachment remains backend follow-up work.
