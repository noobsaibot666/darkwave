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
- Queued export state for offline source paths and ready original-copy execution.
- Traceability flags for preserving source and license records.
- Project source reports include attribution, restriction notes, and source receipt paths.
- CSV rendering for project source/license reports.
- License assessment warnings for missing, uncertain, or expired source/license context.

The planner remains separate from execution. Original-file copies can be executed directly or held in a queue until offline source paths become available. WAV conversion, ranged export rendering, and external drag payload creation remain explicit backend follow-up work.
