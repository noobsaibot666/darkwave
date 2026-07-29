# ADR 0011: Workspace interaction state

## Status

Accepted.

## Context

The listening browser needs fast keyboard navigation, large multi-selection, and drag targets for classification/export. These behaviors should not be embedded only in React event handlers because they affect playback, organization, export, and accessibility.

## Decision

Darkwave models browser focus, selection, range selection, additive selection, select-all-visible, and drag payload creation in `workspace-state`.

Arrow navigation that replaces selection is represented separately from focus movement used by Shift/Cmd interactions. Drag payloads preserve the visible result order of selected assets.

## Consequences

- Selection behavior can be tested independently from rendering.
- Drag-to-classify and external export can share a common selected asset payload.
- The frontend can bind pointer and keyboard events to deterministic commands.
