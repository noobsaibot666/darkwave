# ADR 0001: Cross-platform desktop stack

## Status

Accepted

## Context

The product needs Apple-quality interaction on macOS and a native-feeling Windows build. A SwiftUI-only application would not satisfy the Windows requirement, while a generic browser application would weaken file, playback, and drag-and-drop integration.

## Decision

Use Tauri 2 for the desktop shell, React and TypeScript for the interface, and Rust crates for the local-first core: file import, audio work, storage, search, synchronization, and export.

## Consequences

- The UI can share product behavior across platforms while adapting platform details.
- Rust crates can be tested independently from the desktop shell.
- Tauri permissions must remain intentionally narrow.
- Distribution, code signing, updater, and codec packaging need explicit platform work before release.
