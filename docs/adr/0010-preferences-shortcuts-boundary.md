# ADR 0010: Preferences and shortcuts boundary

## Status

Accepted.

## Context

The MVP requires settings and keyboard-led workflows. Shortcuts must be complete enough for auditioning, import, search, favorites, export, and command palette use without pointer interaction.

## Decision

Darkwave owns user-facing defaults in the `preferences` crate. The crate models browser density, preview cache limits, output device selection, and shortcut bindings. Shortcut validation reports conflicting accelerators before they can be saved.

## Consequences

- Desktop commands can expose one default preferences payload across platforms.
- Shortcut conflicts are detected in core code instead of only in UI state.
- Future persistence can store this payload as user settings without changing the command model.
