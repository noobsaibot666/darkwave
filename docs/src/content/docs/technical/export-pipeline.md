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
- Traceability flags for preserving source and license records.
- License assessment warnings for missing, uncertain, or expired source/license context.

The plan is intentionally separate from execution. File copying, drag payload creation, and audio conversion will be implemented on top of this contract.
