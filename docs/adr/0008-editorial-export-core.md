# ADR 0008: Editorial export core

## Status

Accepted

## Context

Milestone 6 requires external drag-and-drop, copy to project media folder, export presets, optional WAV conversion, usage history, and project source/license reports.

## Decision

Introduce an export planning layer in `export-pipeline` and traceability persistence in `storage`:

- Export requests produce non-destructive plans.
- Original exports preserve the source path and file extension.
- WAV 48 kHz/24-bit exports plan conversion without mutating the library original.
- Export ranges must have positive duration.
- Usage events record exported, dragged, copied, played, and used asset actions.
- Source records can be attached to assets.
- Project source/license reports join project usage events with asset and source metadata.

## Consequences

- The UI can plan export/copy actions before invoking filesystem or conversion code.
- Traceability is available before direct NLE integrations exist.
- Actual copy execution, external drag payloads, FFmpeg conversion, and report file rendering remain future Milestone 6 work.
